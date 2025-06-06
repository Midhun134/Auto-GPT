use crate::ai_functions::ai_fn_backend::{
    print_backend_webserver_code, print_fixed_code, print_improved_webserver_code,
    print_rest_api_endpoints,
};
use crate::helpers::general::{
    check_status_code, read_code_template_contents, read_exec_main_contents, save_api_endpoints,
    save_backend_code, WEB_SERVER_PROJECT_PATH
};

use crate::helpers::command_line::{confirm_safe_code, PrintCommand};
use crate::helpers::general::ai_task_request;
use crate::models::agent_basic::basic_agent::{AgentState, BasicAgent};
use crate::models::agents::agent_traits::{FactSheet, RouteObject, SpecialFunctions};

use async_trait::async_trait;
use reqwest::Client;
use std::fmt::format;
use std::process::{Command, Stdio};
use std::time::Duration;
use tokio::time;


#[derive(Debug)]

pub struct AgentBackendDeveloper {
    attributes: BasicAgent,
    bug_errors: Option<String>,
    bug_count: u8,

}

impl AgentBackendDeveloper {
    pub fn new() -> Self {
        let attributes: BasicAgent = BasicAgent {
            objective: "Develops backend code for webserver and json database".to_string(),
            position: "Backend Developer".to_string(),
            state: AgentState::Discovery,
            memory: vec![]
        };

        Self {
            attributes,
            bug_errors: None,
            bug_count: 0
        }

    }

    async fn call_initial_backend_code(&mut self, factsheet: &mut FactSheet) {
        let code_template_str: String = read_code_template_contents();

        //Concatenate instruction
        let msg_context: String = format!{
            "CODE TEMPLE: {} \n PROJECT_DESCRIPTION: {} \n",
            code_template_str, factsheet.project_description
        };

        let ai_response: String = ai_task_request(
            msg_context, 
            &self.attributes.position, 
            get_function_string!(print_backend_webserver_code),
            print_backend_webserver_code,
            ).await;

        save_backend_code(&ai_response);
        factsheet.backend_code = Some(ai_response);
    }

    async fn call_improved_backend_code(&mut self, factsheet: &mut FactSheet) {

        //Concatenate instruction
        let msg_context: String = format!{
            "CODE TEMPLE: {:?} \n PROJECT_DESCRIPTION: {:?} \n",
            factsheet.backend_code, factsheet
        };

        let mut ai_response: String = ai_task_request(
            msg_context, 
            &self.attributes.position, 
            get_function_string!(print_improved_webserver_code),
            print_improved_webserver_code,
            ).await;
        // remove unwanted comments in ai response
        ai_response = ai_response.replace("```rust", "");
        ai_response = ai_response.replace("```", "");

        save_backend_code(&ai_response);
        factsheet.backend_code = Some(ai_response);
    }

    async fn call_fix_code_bugs(&mut self, factsheet: &mut FactSheet) {
        let msg_context: String = format!(
            "BROKEN_CODE: {:?} \n ERROR_BUGS: {:?} \n
      THIS FUNCTION ONLY OUTPUTS CODE. JUST OUTPUT THE CODE.",
            factsheet.backend_code, self.bug_errors
        );

        let ai_response: String = ai_task_request(
            msg_context,
            &self.attributes.position,
            get_function_string!(print_fixed_code),
            print_fixed_code,
        )
        .await;

        save_backend_code(&ai_response);
        factsheet.backend_code = Some(ai_response);
    }

    async fn call_extract_rest_api_endpoints(&self) -> String {
        let backend_code = read_exec_main_contents();

        //structure message context
        let msg_context = format!("CODE_INPUT: {}", backend_code);
        let ai_response: String = ai_task_request(
            msg_context,
            &self.attributes.position,
            get_function_string!(print_rest_api_endpoints),
            print_rest_api_endpoints,
        )
        .await; 

        ai_response
    }
}

#[async_trait]
impl SpecialFunctions for AgentBackendDeveloper {
    fn get_attributes_from_agent(&self) -> &BasicAgent {
        &self.attributes
    }

    async fn execute(
        &mut self,
        factsheet: &mut FactSheet,) -> Result<(), Box<dyn std::error::Error>> {
            while self.attributes.state != AgentState::Finished {
                match &self.attributes.state {
                    AgentState::Discovery => {
                        self.call_initial_backend_code(factsheet).await;
                        self.attributes.state = AgentState::Working;
                        continue;
                    }
                    AgentState::Working => {
                        if self.bug_count == 0 {
                            self.call_improved_backend_code(factsheet).await;
                        
                        } else {
                            self.call_fix_code_bugs(factsheet).await;
                            

                        }
                        self.attributes.state = AgentState::UnitTesting;
                            continue;
                    }
                    AgentState::UnitTesting => {

                        // Guard: ENsure AI safety
                        PrintCommand::UnitTest.print_agent_message(self.attributes.position.as_str(), "Backend Code unit testing: Requesting user input");
                        let is_safe_code = confirm_safe_code();
                        if !is_safe_code {
                            panic!("Better go work on some AI alignment instead!!!")
                        }

                        //Build and test code
                        PrintCommand::UnitTest.print_agent_message(self.attributes.position.as_str(), 
                        "Backend Code unit testing: building project...");
                        //Build code
                        let build_backend_server: std::process::Output = Command::new("cargo")
                        //This spawns a new process using Rust’s std::process::Command module.
                        .arg("build") //Adds "build" as an argument to cargo, making it execute cargo build
                        .current_dir(WEB_SERVER_PROJECT_PATH)
                        .stdout(Stdio::piped()) // Captures the standard output (stdout) of the process.
                        .stderr(Stdio::piped()) // Captures the standard error (stderr) of the process.
                        .output() //Executes the command and waits for it to complete.
                        .expect("failed to build backend application");
                        // determin if build errors
                        if build_backend_server.status.success() {
                            self.bug_count = 0;
                            PrintCommand::UnitTest.print_agent_message(&self.attributes.position.as_str(),
                             "Backend Code unit testing: Test server build successful...");
                        } else {
                            let error_arr = build_backend_server.stderr;
                            let error_str = String::from_utf8(error_arr).unwrap();

                            //update error stats
                            self.bug_count += 1;
                            self.bug_errors = Some(error_str);

                            //exit if too many bugs
                            if self.bug_count > 2{
                                PrintCommand::Issue.print_agent_message(&self.attributes.position.as_str(), 
                                "Backend Code unit testing: Too many bugs found in code");
                                panic!("Error: Too many bugs!!")
                            }

                            //pass back for rework
                            self.attributes.state = AgentState::Working;
                            continue;
                        }

                        /*
                        extract and test
                        REST API Endpoints
                         */

                        // Extract API Endpoints
                        let api_endpoints_str = self.call_extract_rest_api_endpoints().await;
                        
                        // Convert API Endpoints into values
                        let api_endpoints: Vec<RouteObject> = serde_json::from_str(api_endpoints_str.as_str())
                        .expect("Failed to decode API endpoints");
                        
                        // Define endpoints to check
                        let check_endpoints: Vec<RouteObject> = api_endpoints
                        .iter() //Uses .iter() to iterate over api_endpoints.
                        //Uses .filter(|&route_object| {...}) to select routes meeting the conditions.
                        .filter(|&route_object| {
                            route_object.method == "get" && route_object.is_route_dynamic == "false"
                        })
                        .cloned() //is needed because .iter() gives a reference, and we need owned copies of RouteObject.
                        .collect(); //converts the filtered iterator into a vector (Vec<RouteObject>).

                    //Store API Endpoints
                    factsheet.api_endpoint_schema = Some(check_endpoints.clone());

                    // Build backend application
                    PrintCommand::UnitTest.print_agent_message(&self.attributes.position.as_str(), 
                                "Backend Code unit testing: Starting webserver...");
                    
                    // Execute running server
                    let mut run_backend_server: std::process::Child = Command::new("cargo")
                    .arg("run")
                    .current_dir(WEB_SERVER_PROJECT_PATH)
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                    .expect("failed to run backend application");

                    // Let user know testing on server will take place soon
                    PrintCommand::UnitTest.print_agent_message(&self.attributes.position.as_str(), 
                                "Backend Code unit testing: Launching tests on server in 5secs...");

                    let seconds_sleep = Duration::from_secs(5);
                    time::sleep(seconds_sleep).await;

                    //check status code
                    for endpoint in check_endpoints{
                       
                       //confirm url testing
                       let testing_msg: String = format!("Testing endpoint '{}'...", endpoint.route);
                       PrintCommand::UnitTest.print_agent_message(self.attributes.position.as_str(), 
                                testing_msg.as_str());
                        
                        // Create client with timout
                        let client: Client = Client::builder()
                        .timeout(Duration::from_secs(5))
                        .build()
                        .unwrap();

                        //Test url
                        let url = format!("https://localhost:8080{}", endpoint.route);
                        match check_status_code(&client, &url).await {
                            Ok(status_code) => {
                                if status_code != 200{
                                    let err_msg: String = format!("WARNING: Failed to call backend url endpoint {}", endpoint.route);
                                    PrintCommand::Issue.print_agent_message(self.attributes.position.as_str(), 
                                err_msg.as_str());
                                }
                            }
                        
                        Err(e) => {
                            run_backend_server
                            .kill()
                            .expect("Failed to kill backend server");
                        let err_msg = format!("Error checking backend {}", e);
                        PrintCommand::Issue.print_agent_message(
                            self.attributes.position.as_str(),
                            err_msg.as_str(),
                        );

                        }
                    }
                }

                save_api_endpoints(&api_endpoints_str);

                PrintCommand::UnitTest.print_agent_message(
                    self.attributes.position.as_str(),
                    "Backend testing complete...",
                );

                run_backend_server
                .kill()
                .expect("Failed to kill backend web server on completion");

            self.attributes.state = AgentState::Finished;


                    
                        
                        
            }
                    _=> {}

                }
            }
            Ok(())

        }

    }

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn tests_backend_developer() {
        let mut agent = AgentBackendDeveloper::new();
        let factsheet_str: &str = r#"
        {
            "project_description": "build a personal website",
            "project_scope": {
              "is_crud_required": false,
              "is_user_login_and_logout": true,
              "is_external_urls_required": true
            },
            "external_urls": [],
            "backend_code": null,
            "api_endpoint_schema": null
          }
        "#;

        let mut factsheet: FactSheet = serde_json::from_str(factsheet_str).unwrap(); //there is a mut over here

        //println!("{:#?}", factsheet);
        agent.attributes.state = AgentState::Discovery;
        agent.execute(&mut factsheet).await.expect("failed to execute backend developer agent")
        
    }
}

