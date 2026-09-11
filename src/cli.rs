use clap::{Args, Parser, Subcommand, ValueEnum};

/// Release builds stamp the calendar version (YYYY.MM.DD.HHMM, from the git
/// tag) via the RM_VERSION build-time env; dev builds fall back to the Cargo
/// package version.
pub const VERSION: &str = match option_env!("RM_VERSION") {
    Some(v) => v,
    None => env!("CARGO_PKG_VERSION"),
};

#[derive(Parser)]
#[command(name = "reportmateutil", version = VERSION, about = "ReportMate admin CLI — query and manage your device fleet")]
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
    /// Show a device (all modules), or run a per-device subcommand
    Device {
        /// Device serial number
        serial: String,
        /// Show only one module document (shorthand for `device SERIAL module NAME`)
        #[arg(long)]
        module: Option<String>,
        #[command(subcommand)]
        command: Option<DeviceCommand>,
    },
    /// Fleet-wide report for any module (hardware, applications, installs,
    /// network, security, management, inventory, system, peripherals,
    /// identity, profiles — plus variants like installs/full)
    Module {
        /// Module path under /api/v1/ (e.g. "hardware", "installs/full")
        name: String,
        /// Include archived devices
        #[arg(long)]
        include_archived: bool,
        /// Maximum items to return (1-5000)
        #[arg(long)]
        limit: Option<u32>,
        /// Number of items to skip
        #[arg(long)]
        offset: Option<u32>,
        /// Extra query parameters as key=value (repeatable)
        #[arg(long = "param", value_name = "KEY=VALUE")]
        params: Vec<String>,
    },
    /// Application inventory, usage and distribution reports
    #[command(subcommand, visible_alias = "applications")]
    Apps(Box<AppsCommand>),
    /// Search certificates across the fleet
    #[command(visible_alias = "certs")]
    Certificates {
        /// Match against commonName, issuer, subject or serialNumber
        #[arg(long, default_value = "")]
        search: String,
        /// all, valid, expired, expiring
        #[arg(long, default_value = "all")]
        status: String,
        /// Maximum results
        #[arg(long)]
        limit: Option<u32>,
        /// Include archived devices
        #[arg(long)]
        include_archived: bool,
    },
    /// Sweep one tool's log tails across the fleet (e.g. munki, cimian)
    Logs {
        /// Log root / tool name as reported by the management module
        tool: String,
        /// Comma-separated levels: error, warning, info, debug (default error,warning)
        #[arg(long)]
        levels: Option<String>,
        /// windows or macos
        #[arg(long)]
        platform: Option<String>,
        /// Only lines from this file within the root, e.g. run.log
        #[arg(long)]
        file: Option<String>,
        /// Case-insensitive substring the line must contain
        #[arg(long)]
        grep: Option<String>,
        /// Also return fleet-wide message patterns
        #[arg(long)]
        summary: bool,
        /// Maximum lines per device (1-5000)
        #[arg(long)]
        max_lines_per_device: Option<u32>,
        /// Include archived devices
        #[arg(long)]
        include_archived: bool,
        /// Maximum devices to return (1-5000)
        #[arg(long)]
        limit: Option<u32>,
        /// Number of devices to skip
        #[arg(long)]
        offset: Option<u32>,
    },
    /// Recent fleet events, or an events subcommand
    Events {
        /// Maximum events to return (1-1000)
        #[arg(long)]
        limit: Option<u32>,
        /// Number of events to skip
        #[arg(long)]
        offset: Option<u32>,
        /// Only events after this ISO8601 date
        #[arg(long)]
        since: Option<String>,
        /// Only events before this ISO8601 date
        #[arg(long)]
        until: Option<String>,
        /// Filter by type: success, warning, error, info, system
        #[arg(long = "type")]
        kind: Option<String>,
        #[command(subcommand)]
        command: Option<EventsCommand>,
    },
    /// Dashboard summary (fleet counts, recent events, module rollups)
    Dashboard {
        /// Maximum recent events to include (1-500)
        #[arg(long)]
        events_limit: Option<u32>,
        /// Include archived devices
        #[arg(long)]
        include_archived: bool,
    },
    /// API health (liveness by default)
    /// Show the endpoint and credential the CLI would use, and where each came from
    Config,
    Health {
        /// Readiness probe (database connectivity)
        #[arg(long, conflicts_with = "full")]
        ready: bool,
        /// Full health report
        #[arg(long)]
        full: bool,
    },
    /// Prometheus metrics exposition (text)
    Metrics,
    /// Real-time (SignalR / Web PubSub) negotiate: connection URL and token
    Negotiate {
        /// Client identity to negotiate as
        #[arg(long, default_value = "dashboard")]
        device: String,
    },
    /// Archive a device (alias for `device SERIAL archive`)
    #[command(hide = true)]
    Archive { serial: String },
    /// Unarchive a device (alias for `device SERIAL unarchive`)
    #[command(hide = true)]
    Unarchive { serial: String },
    /// Delete a device (alias for `device SERIAL delete`)
    #[command(hide = true)]
    Delete {
        serial: String,
        #[arg(long)]
        confirm: bool,
    },
    /// Manage per-client API keys (admin scope)
    #[command(subcommand)]
    ApiKeys(ApiKeysCommand),
    /// Administrative maintenance and diagnostics (admin scope)
    #[command(subcommand)]
    Admin(AdminCommand),
    /// Read or write server-side org settings
    #[command(subcommand)]
    Settings(SettingsCommand),
    /// Call any /api/v1 path with any method and print the response (escape hatch)
    Raw {
        /// Path under the API root, e.g. /api/v1/dashboard
        path: String,
        /// HTTP method
        #[arg(long, short = 'X', value_enum, default_value_t = HttpMethod::Get)]
        method: HttpMethod,
        /// JSON request body, or @path to read it from a file
        #[arg(long, short = 'd')]
        data: Option<String>,
        /// Extra query parameters as key=value (repeatable)
        #[arg(long = "param", value_name = "KEY=VALUE")]
        params: Vec<String>,
    },
}

#[derive(Subcommand)]
pub enum DeviceCommand {
    /// Fast summary (identity, platform, last seen) without module payloads
    Info,
    /// One module document (hardware, installs, network, ...)
    Module {
        /// Module name
        name: String,
    },
    /// Events recorded for this device
    Events {
        /// Maximum events to return
        #[arg(long)]
        limit: Option<u32>,
        /// Filter by type: success, warning, error, info, system
        #[arg(long = "type")]
        kind: Option<String>,
    },
    /// Managed-software (Munki/Cimian) install log
    InstallsLog,
    /// Log tail for one tool root (e.g. munki, cimian, intune)
    Log {
        /// Tool / log root name
        tool: String,
    },
    /// Application usage history for this device
    Usage {
        /// Days to look back (1-548)
        #[arg(long)]
        days: Option<u32>,
        /// Filter by application name
        #[arg(long)]
        app: Option<String>,
    },
    /// Archive the device (admin scope)
    Archive,
    /// Unarchive the device (admin scope)
    Unarchive,
    /// Permanently delete the device and all its data (admin scope)
    Delete {
        /// Required to actually delete — without it the call is refused
        #[arg(long)]
        confirm: bool,
    },
}

#[derive(Subcommand)]
pub enum EventsCommand {
    /// Ingest failures: check-ins the API turned away
    Failures {
        /// Maximum failures to return (1-1000)
        #[arg(long)]
        limit: Option<u32>,
        /// Number of failures to skip
        #[arg(long)]
        offset: Option<u32>,
        /// Filter by serial number (case-insensitive substring)
        #[arg(long)]
        serial: Option<String>,
        /// Filter by rejection reason code
        #[arg(long)]
        reason: Option<String>,
        /// Look-back window in hours (1-2160, default 168)
        #[arg(long)]
        hours: Option<u32>,
        /// rejected (default), retried, accepted, or all
        #[arg(long)]
        outcome: Option<String>,
    },
    /// Full payload for one event, including the related module data
    Payload {
        /// Event id
        event_id: u64,
    },
    /// Submit a device check-in payload (what the device agents POST)
    Submit {
        /// Payload as JSON, or @path to read from a file
        json: String,
    },
}

/// Inventory-facet filters shared by the applications reports. Each takes a
/// comma-separated list.
#[derive(Args, Default)]
pub struct InventoryFilters {
    /// Inventory usages
    #[arg(long)]
    pub usages: Option<String>,
    /// Inventory catalogs
    #[arg(long)]
    pub catalogs: Option<String>,
    /// Inventory locations
    #[arg(long)]
    pub locations: Option<String>,
    /// Inventory areas (departments)
    #[arg(long)]
    pub areas: Option<String>,
    /// Inventory fleets
    #[arg(long)]
    pub fleets: Option<String>,
    /// Inventory rooms
    #[arg(long)]
    pub rooms: Option<String>,
    /// Platforms (windows, macos)
    #[arg(long)]
    pub platforms: Option<String>,
}

#[derive(Subcommand)]
pub enum AppsCommand {
    /// Installed applications across the fleet, with filters
    List {
        /// Comma-separated device names
        #[arg(long)]
        devices: Option<String>,
        /// Comma-separated application names
        #[arg(long)]
        names: Option<String>,
        /// Comma-separated publishers
        #[arg(long)]
        publishers: Option<String>,
        /// Comma-separated categories
        #[arg(long)]
        categories: Option<String>,
        /// Comma-separated versions
        #[arg(long)]
        versions: Option<String>,
        /// Free-text search
        #[arg(long)]
        search: Option<String>,
        /// Installed on or after this date
        #[arg(long)]
        installed_from: Option<String>,
        /// Installed on or before this date
        #[arg(long)]
        installed_to: Option<String>,
        /// Minimum size in bytes
        #[arg(long)]
        size_min: Option<u64>,
        /// Maximum size in bytes
        #[arg(long)]
        size_max: Option<u64>,
        #[command(flatten)]
        filters: InventoryFilters,
        /// Return every item instead of one page
        #[arg(long)]
        load_all: bool,
        /// Include archived devices
        #[arg(long)]
        include_archived: bool,
        /// Maximum items to return (1-5000, default 500)
        #[arg(long)]
        limit: Option<u32>,
        /// Number of items to skip
        #[arg(long)]
        offset: Option<u32>,
        /// Maximum devices to scan (1-5000, default 2000)
        #[arg(long)]
        device_limit: Option<u32>,
    },
    /// Available filter values (publishers, categories, inventory facets)
    Filters {
        /// Include archived devices
        #[arg(long)]
        include_archived: bool,
    },
    /// Fleet-wide application usage over a window
    Usage {
        /// Lookback window in days (1-548, default 30)
        #[arg(long)]
        days: Option<u32>,
        /// Comma-separated application names to include
        #[arg(long)]
        names: Option<String>,
        /// Minimum total hours to include an app
        #[arg(long)]
        min_hours: Option<f64>,
        /// Minimum launch count to include an app
        #[arg(long)]
        min_launches: Option<u32>,
        #[command(flatten)]
        filters: InventoryFilters,
        /// Include archived devices
        #[arg(long)]
        include_archived: bool,
    },
    /// Per-device usage of one application (name is a substring match)
    ByDevice {
        /// Application name pattern
        app: String,
        /// Lookback window in days (1-548, default 30)
        #[arg(long)]
        days: Option<u32>,
        /// Comma-separated inventory usages
        #[arg(long)]
        usages: Option<String>,
        /// Comma-separated inventory catalogs
        #[arg(long)]
        catalogs: Option<String>,
        /// Comma-separated inventory locations
        #[arg(long)]
        locations: Option<String>,
        /// Include archived devices
        #[arg(long)]
        include_archived: bool,
    },
    /// Where named applications are installed, aggregated by inventory facet
    Distribution {
        /// Comma-separated application names to aggregate
        names: String,
        #[command(flatten)]
        filters: InventoryFilters,
        /// Include archived devices
        #[arg(long)]
        include_archived: bool,
    },
    /// Freshness of application data collection across the fleet
    CollectionHealth {
        /// Days within which a device is considered healthy (1-90, default 7)
        #[arg(long)]
        fresh_days: Option<u32>,
        /// Days beyond which a device is considered dark (1-180, default 30)
        #[arg(long)]
        stale_days: Option<u32>,
        /// Include archived devices
        #[arg(long)]
        include_archived: bool,
    },
}

#[derive(Subcommand)]
pub enum AdminCommand {
    /// usage_history maintenance: anomalies, integrity, export, baseline, retention
    #[command(subcommand)]
    UsageHistory(UsageHistoryCommand),
    /// Managed-software installs maintenance
    #[command(subcommand)]
    Installs(InstallsCommand),
    /// Database diagnostics: duplicates, orphans, retention, bloat
    DebugDatabase,
    /// Alias for `admin usage-history cleanup`
    #[command(hide = true)]
    CleanupUsage {
        #[arg(long, default_value_t = 18)]
        months: u32,
    },
    /// Alias for `admin installs clear-errors`
    #[command(hide = true)]
    ClearErrors {
        #[arg(long, default_value_t = 10)]
        days: u32,
    },
}

#[derive(Subcommand)]
pub enum UsageHistoryCommand {
    /// Rows whose date could not have come from a healthy client
    DateAnomalies {
        /// Rows dated before this YYYY-MM-DD are implausible (default: 548 days ago)
        #[arg(long)]
        floor: Option<String>,
        /// Maximum sample rows per bucket (1-1000)
        #[arg(long)]
        limit: Option<u32>,
    },
    /// Physical-plausibility check over recent rows, per platform
    Integrity {
        /// Lookback window in days (1-90, default 7)
        #[arg(long)]
        days: Option<u32>,
        /// Maximum offending device-days returned (1-200, default 20)
        #[arg(long)]
        sample: Option<u32>,
    },
    /// Export rows for a date range as CSV (half-open: from inclusive, to exclusive)
    Export {
        /// First row date to include, YYYY-MM-DD
        #[arg(long)]
        from: String,
        /// First row date to exclude, YYYY-MM-DD
        #[arg(long)]
        to: String,
        /// Write the CSV to this file instead of stdout
        #[arg(long, short = 'o')]
        out: Option<String>,
    },
    /// Archive and remove rows dated before a day (preview unless --confirm)
    ResetBaseline {
        /// Archive and remove rows dated before this YYYY-MM-DD (exclusive)
        #[arg(long)]
        before: String,
        /// Execute; without it a preview is returned
        #[arg(long)]
        confirm: bool,
        /// Recorded on the archived rows so the batch can be identified later
        #[arg(long)]
        reason: Option<String>,
    },
    /// Delete rows older than the retention window
    Cleanup {
        /// Retain data for this many months (1-36)
        #[arg(long, default_value_t = 18)]
        months: u32,
    },
}

#[derive(Subcommand)]
pub enum InstallsCommand {
    /// Clear stale install errors/warnings
    ClearErrors {
        /// Clear for devices not seen in this many days (0-365; 0 clears every device)
        #[arg(long, default_value_t = 10)]
        days: u32,
        /// Also clear individual items whose own last attempt is older than this many days
        #[arg(long)]
        item_age_days: Option<f64>,
    },
    /// Re-run the install-item classifier over stored installs rows
    Reclassify {
        /// Rows to read per batch (1-1000, default 200)
        #[arg(long)]
        batch: Option<u32>,
        /// Stop after this many rows (rehearsal against part of the fleet)
        #[arg(long)]
        limit: Option<u32>,
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
        /// Actor recorded on the settings revision (X-Updated-By)
        #[arg(long)]
        updated_by: Option<String>,
    },
    /// Discover inventory keys present in the fleet (requires
    /// REPORTMATE_INTERNAL_SECRET)
    Discover {
        /// Include archived devices
        #[arg(long)]
        include_archived: bool,
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

#[derive(Copy, Clone, ValueEnum)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}
