use clap::{Parser, Subcommand, ValueEnum};

/// Release builds stamp the calendar version (YYYY.MM.DD.HHMM, from the git
/// tag) via the RM_VERSION build-time env; dev builds fall back to the Cargo
/// package version.
pub const VERSION: &str = match option_env!("RM_VERSION") {
    Some(v) => v,
    None => env!("CARGO_PKG_VERSION"),
};

#[derive(Parser)]
#[command(name = "reportmate", version = VERSION, about = "ReportMate admin CLI — query and manage your device fleet")]
pub struct Cli {
    /// Output format (tables for humans, json for scripts and agents)
    #[arg(long, value_enum, default_value_t = OutputFormat::Table, global = true)]
    pub output: OutputFormat,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// List devices in the fleet
    Devices {
        /// Maximum devices to return (1-1000)
        #[arg(long)]
        limit: Option<u32>,
        /// Pagination offset
        #[arg(long)]
        offset: Option<u32>,
        /// Include archived devices
        #[arg(long)]
        include_archived: bool,
    },
    /// Show a single device by serial number (all modules)
    Device {
        /// Device serial number
        serial: String,
        /// Show only one module document (e.g. hardware, installs, network)
        #[arg(long)]
        module: Option<String>,
    },
    /// Fleet-wide report for any module (hardware, applications, installs,
    /// network, security, management, inventory, system, peripherals,
    /// identity — plus variants like installs/full or security/certificates)
    Module {
        /// Module path under /api/v1/ (e.g. "hardware", "installs/full")
        name: String,
        /// Extra query parameters as key=value (repeatable)
        #[arg(long = "param", value_name = "KEY=VALUE")]
        params: Vec<String>,
    },
    /// Recent fleet events
    Events {
        /// Maximum events to return
        #[arg(long)]
        limit: Option<u32>,
    },
    /// API health (liveness; --ready adds the database probe)
    Health {
        /// Check readiness (database connectivity) instead of liveness
        #[arg(long)]
        ready: bool,
    },
    /// Archive a device (admin scope)
    Archive {
        /// Device serial number
        serial: String,
    },
    /// Unarchive a device (admin scope)
    Unarchive {
        /// Device serial number
        serial: String,
    },
    /// Permanently delete a device and all its data (admin scope)
    Delete {
        /// Device serial number
        serial: String,
        /// Required to actually delete — without it the call is refused
        #[arg(long)]
        confirm: bool,
    },
    /// Manage per-client API keys (admin scope)
    #[command(subcommand)]
    ApiKeys(ApiKeysCommand),
    /// Administrative maintenance operations (admin scope)
    #[command(subcommand)]
    Admin(AdminCommand),
    /// Read or write server-side org settings
    #[command(subcommand)]
    Settings(SettingsCommand),
    /// GET any /api/v1 path and print the JSON (escape hatch)
    Raw {
        /// Path under the API root, e.g. /api/v1/dashboard
        path: String,
    },
}

#[derive(Subcommand)]
pub enum AdminCommand {
    /// Delete usage-history rows older than the retention window
    CleanupUsage {
        /// Retain data for this many months (1-36)
        #[arg(long, default_value_t = 18)]
        months: u32,
    },
    /// Clear stale install errors/warnings from long-absent devices
    ClearErrors {
        /// Clear for devices not seen in this many days (1-365)
        #[arg(long, default_value_t = 10)]
        days: u32,
    },
}

#[derive(Subcommand)]
pub enum SettingsCommand {
    /// Show the current org settings
    Get,
    /// Replace the org settings with a JSON document (requires
    /// REPORTMATE_INTERNAL_SECRET)
    Set {
        /// Settings as a JSON object, or @path to read from a file
        json: String,
    },
}

#[derive(Subcommand)]
pub enum ApiKeysCommand {
    /// List issued API keys
    List,
    /// Create a key: prints the secret once
    Create {
        /// Client id / owner label for the key
        name: String,
        /// Scopes (read, ingest, admin; repeatable)
        #[arg(long = "scope", default_values_t = vec![String::from("read")])]
        scopes: Vec<String>,
    },
    /// Revoke a key by id
    Revoke {
        /// Key id
        key_id: String,
    },
}

#[derive(Copy, Clone, ValueEnum)]
pub enum OutputFormat {
    Table,
    Json,
}
