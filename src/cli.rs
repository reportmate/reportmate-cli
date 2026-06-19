use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(name = "reportmate", version, about = "ReportMate admin CLI")]
pub struct Cli {
    /// Output format
    #[arg(long, value_enum, default_value_t = OutputFormat::Table, global = true)]
    pub output: OutputFormat,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// List all devices in the fleet
    Devices,
    /// Show a single device by serial number
    Device {
        /// Device serial number
        serial: String,
    },
    /// Fleet-wide applications report
    Applications,
}

#[derive(Copy, Clone, ValueEnum)]
pub enum OutputFormat {
    Table,
    Json,
}
