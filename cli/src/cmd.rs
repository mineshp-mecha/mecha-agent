use std::{io, time::Duration};

use anyhow::Result;
use clap::{command, Args, Parser, Subcommand};
use identity::service::get_machine_details;
use provisioning::service::{de_provision, generate_code, provision_by_code};
use settings::service::get_settings_by_key;
use tokio::time::{self, Instant};

#[derive(Debug, Clone)]
pub struct Settings {
    pub data_dir: String,
    pub service_url: String,
}

#[derive(Debug, Args)]
#[command(about = "configure API Key for your account")]
pub struct Setup {
    /// Import from filepath
    #[arg(short = 'i', long = "import", value_name = "filepath")]
    import: Vec<String>,
}

impl Setup {
    pub async fn run(&self) -> Result<()> {
        // Check if 'configure' is the only command provided
        let code = match generate_code() {
            Ok(code) => code,
            Err(e) => {
                eprintln!("Error: {}", e);
                return Err(e);
            }
        };
        println!("code generated: {:?}", code);
        // Waiting for provisioning ...
        let interval_duration = Duration::from_secs(10);
        let interval_total_duration = Duration::from_secs(60);
        let mut interval = time::interval_at(Instant::now() + interval_duration, interval_duration);
        let mut total_duration = time::interval_at(
            Instant::now() + interval_total_duration,
            interval_total_duration,
        );

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    println!("Waiting for provisioning ...: TICK DURATION");
                    match provision_by_code(code.clone(), None).await {
                        Ok(_) => {
                            println!("Provisioning successful ...: PROVISIONING SUCCESSFUL");
                            break;
                        }
                        Err(e) => {
                            eprintln!("Error: {}", e);
                        }
                    };
                }
                _ = total_duration.tick() => {
                    println!("Total duration of 60 seconds has elapsed ...: TOTAL DURATION");
                    println!("Request timed out.");
                    break;
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Args)]
#[command(about = "machine details")]
pub struct Whoami {
    /// Import from filepath
    #[arg(short = 'w', long = "whoami", value_name = "filepath")]
    import: Vec<String>,
}

impl Whoami {
    pub async fn run(&self) -> Result<()> {
        //TODO: figure how to print machine details on terminal
        let machine_details = match get_machine_details() {
            Ok(v) => v,
            Err(e) => {
                eprintln!("Error: {}", e);
                return Err(e);
            }
        };
        let machine_name = match get_settings_by_key(String::from("identity.machine.name")).await {
            Ok(v) => v,
            Err(e) => {
                eprintln!("Error: {}", e);
                return Err(e);
            }
        };
        let machine_alias = match get_settings_by_key(String::from("identity.machine.alias")).await
        {
            Ok(v) => v,
            Err(e) => {
                eprintln!("Error: {}", e);
                return Err(e);
            }
        };
        println!("Machine Name: {:?}", machine_name);
        println!("Machine Alias: {:?}", machine_alias);
        Ok(())
    }
}

#[derive(Debug, Args)]
#[command(about = "reset machine")]
pub struct Reset {
    /// Import from filepath
    #[arg(short = 'r', long = "rest", value_name = "filepath")]
    import: Vec<String>,
}

impl Reset {
    pub async fn run(&self) -> Result<()> {
        let machine_name = match get_settings_by_key(String::from("identity.machine.name")).await {
            Ok(v) => v,
            Err(e) => {
                eprintln!("Error: {}", e);
                return Err(e);
            }
        };

        use std::io::{stdin, stdout, Write};
        let mut s = String::new();
        print!(
            "Are you sure you want to reset the agent (Name: {}) [Y/N] - ? ",
            machine_name
        );
        let _ = stdout().flush();
        stdin()
            .read_line(&mut s)
            .expect("Did not enter a correct string");
        match s.trim().to_lowercase().as_str() {
            "y" | "yes" => {
                match de_provision(None) {
                    Ok(_) => {
                        println!("De-provisioning successful ...: DE-PROVISIONING SUCCESSFUL");
                    }
                    Err(e) => {
                        eprintln!("Error: {}", e);
                        return Err(e);
                    }
                };
            }
            _ => {
                println!("Reset aborted.");
            }
        }
        Ok(())
    }
}
