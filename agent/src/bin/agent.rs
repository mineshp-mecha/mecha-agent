use agent_settings::{read_settings_yml, AgentSettings};
use anyhow::{bail, Result};
use clap::Parser;
use init_tracing_opentelemetry::tracing_subscriber_ext::{
    build_logger_text, build_loglevel_filter_layer, build_otel_layer,
};

use cli::cmd::{Reset, Setup, Whoami};
use mecha_agent::init::init_handlers;
use opentelemetry::global;
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use telemetry::config::init_logs_config;
use tracing_subscriber::prelude::__tracing_subscriber_SubscriberExt;
const PACKAGE_NAME: &str = env!("CARGO_PKG_NAME");

#[derive(Parser, Debug)]
struct StartCommand {
    /// Path to the settings file
    #[arg(short, long)]
    settings: String,

    #[arg(long = "server")]
    init_grpc: bool,
}
#[derive(Debug, Parser)]
enum MectlCommand {
    #[command(about = "Start the agent")]
    Start(StartCommand),
    #[command(about = "Setup new machine")]
    Setup(Setup),
    #[command(about = "Machine details")]
    Whoami(Whoami),
    #[command(about = "Reset machine")]
    Reset(Reset),
}
#[derive(Debug, Parser)] // requires `derive` feature
#[command(name = "mectl")]
#[command(about = "mecha agent CLI", long_about = None)]
struct Mectl {
    #[command(subcommand)]
    command: MectlCommand,
}
#[tokio::main]
async fn main() -> Result<()> {
    // Setting tracing
    let mectl = Mectl::parse();
    match mectl.command {
        MectlCommand::Setup(configure) => {
            let _ = configure.run().await;
        }
        MectlCommand::Start(start) => {
            println!("Starting agent ... {:?}", start.settings);
            // configure the global logger to use our opentelemetry logger
            let _ = start_agent(start.settings, start.init_grpc).await;
        }
        MectlCommand::Whoami(whoami) => {
            let _ = whoami.run().await;
        }
        MectlCommand::Reset(reset) => {
            let _ = reset.run().await;
        }
        _ => {
            bail!("Command not found");
        }
    }
    Ok(())
}

async fn start_agent(settings_path: String, init_grpc: bool) -> Result<()> {
    let settings = read_settings_yml(Some(settings_path)).unwrap();
    // configure the global logger to use our opentelemetry logger
    let _ = init_logs_config();
    let logger_provider = opentelemetry::global::logger_provider();
    let tracing_bridge_layer = OpenTelemetryTracingBridge::new(&logger_provider);
    global::set_logger_provider(logger_provider);

    let subscriber = tracing_subscriber::registry()
        .with(tracing_bridge_layer)
        .with(build_loglevel_filter_layer()) //temp for terminal log
        .with(build_logger_text()) //temp for terminal log
        .with(build_otel_layer().unwrap());
    match tracing::subscriber::set_global_default(subscriber) {
        Ok(_) => (),
        Err(e) => bail!("Error setting global default subscriber: {}", e),
    };
    tracing::info!(
        //sample log
        func = "set_tracing",
        package = env!("CARGO_PKG_NAME"),
        result = "success",
        "tracing set up",
    );
    let _ = init_handlers(settings, init_grpc).await;
    Ok(())
}
