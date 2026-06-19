mod cli;
mod client;
mod config;
mod output;

use anyhow::Result;
use clap::Parser;

use cli::{Cli, Command, OutputFormat};

#[tokio::main]
async fn main() -> Result<()> {
    let args = Cli::parse();
    let cfg = config::Config::load()?;
    let client = client::Client::new(cfg)?;

    match args.command {
        Command::Devices => {
            let data = client.get("/api/v1/devices").await?;
            match args.output {
                OutputFormat::Json => output::print_json(&data),
                OutputFormat::Table => output::print_devices_table(&data),
            }
        }
        Command::Device { serial } => {
            let data = client.get(&format!("/api/v1/device/{serial}")).await?;
            // Device detail is deeply nested; JSON for now, a richer table view is a TODO.
            output::print_json(&data);
        }
        Command::Applications => {
            let data = client.get("/api/v1/applications").await?;
            match args.output {
                OutputFormat::Json => output::print_json(&data),
                // TODO: a dedicated applications table view.
                OutputFormat::Table => output::print_json(&data),
            }
        }
    }

    Ok(())
}
