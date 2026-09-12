mod cli;
mod client;
mod config;
mod output;

use anyhow::{bail, Context, Result};
use clap::Parser;
use reqwest::Method;
use serde_json::{json, Value};

use cli::{
    AdminCommand, ApiKeysCommand, AppsCommand, Cli, Command, DeviceCommand, EventsCommand,
    HttpMethod, InstallsCommand, InventoryFilters, OutputFormat, SettingsCommand,
    UsageHistoryCommand,
};
use client::{Body, Client};

/// Query-string builder: every setter is a no-op for `None`/`false`, so the
/// CLI only sends what the operator actually asked for and the API's own
/// defaults apply otherwise.
#[derive(Default)]
struct Query(Vec<(String, String)>);

impl Query {
    fn new() -> Query {
        Query::default()
    }

    fn set(mut self, key: &str, value: impl ToString) -> Query {
        self.0.push((key.to_string(), value.to_string()));
        self
    }

    fn opt<T: ToString>(self, key: &str, value: Option<T>) -> Query {
        match value {
            Some(v) => self.set(key, v),
            None => self,
        }
    }

    fn flag(self, key: &str, on: bool) -> Query {
        if on {
            self.set(key, "true")
        } else {
            self
        }
    }

    fn filters(self, f: &InventoryFilters) -> Query {
        self.opt("usages", f.usages.as_ref())
            .opt("catalogs", f.catalogs.as_ref())
            .opt("locations", f.locations.as_ref())
            .opt("areas", f.areas.as_ref())
            .opt("fleets", f.fleets.as_ref())
            .opt("rooms", f.rooms.as_ref())
            .opt("platforms", f.platforms.as_ref())
    }

    /// Append `key=value` pairs given verbatim on the command line.
    fn params(mut self, pairs: &[String]) -> Result<Query> {
        for p in pairs {
            match p.split_once('=') {
                Some((k, v)) => self.0.push((k.to_string(), v.to_string())),
                None => bail!("--param must be key=value, got: {p}"),
            }
        }
        Ok(self)
    }

    fn path(&self, base: &str) -> String {
        format!("{base}{}", query_string(&self.0))
    }
}

fn query_string(pairs: &[(String, String)]) -> String {
    if pairs.is_empty() {
        return String::new();
    }
    let joined: Vec<String> = pairs
        .iter()
        .map(|(k, v)| format!("{}={}", urlencode(k), urlencode(v)))
        .collect();
    format!("?{}", joined.join("&"))
}

fn urlencode(s: &str) -> String {
    s.chars()
        .flat_map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => vec![c.to_string()],
            _ => c
                .to_string()
                .into_bytes()
                .iter()
                .map(|b| format!("%{:02X}", b))
                .collect(),
        })
        .collect()
}

/// A JSON argument: either inline JSON or `@path` to a file.
fn json_arg(arg: String, what: &str) -> Result<Value> {
    let raw = if let Some(path) = arg.strip_prefix('@') {
        std::fs::read_to_string(path).with_context(|| format!("reading {path}"))?
    } else {
        arg
    };
    serde_json::from_str(&raw).map_err(|e| anyhow::anyhow!("{what} must be valid JSON: {e}"))
}

/// Restore the default SIGPIPE disposition.
///
/// Rust ignores SIGPIPE by default, so a consumer that closes stdout early
/// (e.g. `reportmateutil devices | head`) turns the next write into a panic
/// ("failed printing to stdout: Broken pipe") instead of a clean exit.
/// Restoring SIG_DFL makes the process terminate normally, like any Unix tool.
#[cfg(unix)]
fn reset_sigpipe() {
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
}

#[cfg(not(unix))]
fn reset_sigpipe() {}

#[tokio::main]
async fn main() -> Result<()> {
    reset_sigpipe();
    let args = Cli::parse();
    if matches!(args.command, Command::Config) {
        return print_config(args.output).await;
    }
    let cfg = config::Config::load().await?;
    let client = Client::new(cfg)?;

    match args.command {
        Command::Devices {
            limit,
            offset,
            include_archived,
        } => {
            let q = Query::new()
                .opt("limit", limit)
                .opt("offset", offset)
                .flag("includeArchived", include_archived);
            let data = client.get(&q.path("/api/v1/devices")).await?;
            match args.output {
                OutputFormat::Json => output::print_json(&data),
                OutputFormat::Table => output::print_devices_table(&data),
            }
        }
        Command::Device {
            serial,
            module,
            command,
        } => {
            let command = match (module, command) {
                (Some(_), Some(_)) => bail!("--module cannot be combined with a subcommand"),
                (Some(name), None) => Some(DeviceCommand::Module { name }),
                (None, c) => c,
            };
            device(&client, &serial, command).await?;
        }
        Command::Module {
            name,
            include_archived,
            limit,
            offset,
            params,
        } => {
            let q = Query::new()
                .flag("includeArchived", include_archived)
                .opt("limit", limit)
                .opt("offset", offset)
                .params(&params)?;
            let name = name.trim_matches('/');
            let data = client.get(&q.path(&format!("/api/v1/{name}"))).await?;
            output::print_json(&data);
        }
        Command::Apps(cmd) => apps(&client, *cmd).await?,
        Command::Certificates {
            search,
            status,
            limit,
            include_archived,
        } => {
            let q = Query::new()
                .set("search", search)
                .set("status", status)
                .opt("limit", limit)
                .flag("includeArchived", include_archived);
            let data = client.get(&q.path("/api/v1/security/certificates")).await?;
            output::print_json(&data);
        }
        Command::Logs {
            tool,
            levels,
            platform,
            file,
            grep,
            summary,
            max_lines_per_device,
            include_archived,
            limit,
            offset,
        } => {
            let q = Query::new()
                .opt("levels", levels)
                .opt("platform", platform)
                .opt("file", file)
                .opt("q", grep)
                .flag("summary", summary)
                .opt("maxLinesPerDevice", max_lines_per_device)
                .flag("includeArchived", include_archived)
                .opt("limit", limit)
                .opt("offset", offset);
            let data = client
                .get(&q.path(&format!("/api/v1/management/logs/{tool}")))
                .await?;
            output::print_json(&data);
        }
        Command::Events {
            limit,
            offset,
            since,
            until,
            kind,
            command,
        } => match command {
            None => {
                let q = Query::new()
                    .opt("limit", limit)
                    .opt("offset", offset)
                    .opt("startDate", since)
                    .opt("endDate", until)
                    .opt("type", kind);
                let data = client.get(&q.path("/api/v1/events")).await?;
                match args.output {
                    OutputFormat::Json => output::print_json(&data),
                    OutputFormat::Table => output::print_events_table(&data),
                }
            }
            Some(cmd) => events(&client, cmd).await?,
        },
        Command::Dashboard {
            events_limit,
            include_archived,
        } => {
            let q = Query::new()
                .opt("eventsLimit", events_limit)
                .flag("includeArchived", include_archived);
            let data = client.get(&q.path("/api/v1/dashboard")).await?;
            output::print_json(&data);
        }
        Command::Config => unreachable!("handled before the client is built"),
        Command::Health { ready, full } => {
            let path = if ready {
                "/api/v1/health/ready"
            } else if full {
                "/api/v1/health"
            } else {
                "/api/v1/health/live"
            };
            let data = client.get(path).await?;
            output::print_json(&data);
        }
        Command::Metrics => {
            let text = client.get_text("/api/v1/metrics").await?;
            output::print_text(&text);
        }
        Command::Negotiate { device } => {
            let q = Query::new().set("device", device);
            let data = client.get(&q.path("/api/v1/negotiate")).await?;
            output::print_json(&data);
        }
        Command::Archive { serial } => {
            device(&client, &serial, Some(DeviceCommand::Archive)).await?
        }
        Command::Unarchive { serial } => {
            device(&client, &serial, Some(DeviceCommand::Unarchive)).await?
        }
        Command::Delete { serial, confirm } => {
            device(&client, &serial, Some(DeviceCommand::Delete { confirm })).await?
        }
        Command::ApiKeys(cmd) => match cmd {
            ApiKeysCommand::List => {
                let data = client.get("/api/v1/admin/api-keys").await?;
                output::print_json(&data);
            }
            ApiKeysCommand::Create { name, scopes } => {
                let body = json!({ "client_id": name, "scopes": scopes });
                let data = client.post("/api/v1/admin/api-keys", &body).await?;
                output::print_json(&data);
            }
            ApiKeysCommand::Revoke { key_id } => {
                let data = client
                    .delete(&format!("/api/v1/admin/api-keys/{key_id}"))
                    .await?;
                output::print_json(&data);
            }
        },
        Command::Admin(cmd) => admin(&client, cmd).await?,
        Command::Settings(cmd) => match cmd {
            SettingsCommand::Get => {
                let data = client.get("/api/v1/settings").await?;
                output::print_json(&data);
            }
            SettingsCommand::Set { json, updated_by } => {
                let body = json_arg(json, "settings")?;
                let headers: Vec<(&str, &str)> = updated_by
                    .as_deref()
                    .map(|v| vec![("X-Updated-By", v)])
                    .unwrap_or_default();
                let data = client.put("/api/v1/settings", &body, &headers).await?;
                output::print_json(&data);
            }
            SettingsCommand::Discover { include_archived } => {
                let q = Query::new().flag("include_archived", include_archived);
                let data = client
                    .get(&q.path("/api/v1/settings/inventory/discover"))
                    .await?;
                output::print_json(&data);
            }
        },
        Command::Raw {
            path,
            method,
            data,
            params,
        } => {
            if !path.starts_with('/') {
                bail!("path must start with /, e.g. /api/v1/dashboard");
            }
            let method = match method {
                HttpMethod::Get => Method::GET,
                HttpMethod::Post => Method::POST,
                HttpMethod::Put => Method::PUT,
                HttpMethod::Patch => Method::PATCH,
                HttpMethod::Delete => Method::DELETE,
            };
            let body = data.map(|d| json_arg(d, "--data")).transpose()?;
            let full = Query::new().params(&params)?.path(&path);
            match client.request(method, &full, body.as_ref(), &[]).await? {
                Body::Json(v) => output::print_json(&v),
                Body::Text(t) => output::print_text(&t),
            }
        }
    }

    Ok(())
}

async fn device(client: &Client, serial: &str, cmd: Option<DeviceCommand>) -> Result<()> {
    let base = format!("/api/v1/device/{serial}");
    let data = match cmd {
        None => client.get(&base).await?,
        Some(DeviceCommand::Info) => client.get(&format!("{base}/info")).await?,
        Some(DeviceCommand::Module { name }) => {
            client.get(&format!("{base}/modules/{name}")).await?
        }
        Some(DeviceCommand::Events { limit, kind }) => {
            let q = Query::new().opt("limit", limit).opt("type", kind);
            client.get(&q.path(&format!("{base}/events"))).await?
        }
        Some(DeviceCommand::InstallsLog) => client.get(&format!("{base}/installs/log")).await?,
        Some(DeviceCommand::Log { tool }) => client.get(&format!("{base}/logs/{tool}")).await?,
        Some(DeviceCommand::Usage { days, app }) => {
            let q = Query::new().opt("days", days).opt("appName", app);
            client
                .get(&q.path(&format!("{base}/applications/usage/history")))
                .await?
        }
        Some(DeviceCommand::Archive) => client.patch(&format!("{base}/archive")).await?,
        Some(DeviceCommand::Unarchive) => client.patch(&format!("{base}/unarchive")).await?,
        Some(DeviceCommand::Delete { confirm }) => {
            if !confirm {
                bail!("refusing to delete {serial} without --confirm");
            }
            client.delete(&format!("{base}?confirm=true")).await?
        }
    };
    output::print_json(&data);
    Ok(())
}

async fn events(client: &Client, cmd: EventsCommand) -> Result<()> {
    let data = match cmd {
        EventsCommand::Failures {
            limit,
            offset,
            serial,
            reason,
            hours,
            outcome,
        } => {
            let q = Query::new()
                .opt("limit", limit)
                .opt("offset", offset)
                .opt("serial", serial)
                .opt("reason", reason)
                .opt("hours", hours)
                .opt("outcome", outcome);
            client.get(&q.path("/api/v1/events/failures")).await?
        }
        EventsCommand::Payload { event_id } => {
            client
                .get(&format!("/api/v1/events/{event_id}/payload"))
                .await?
        }
        EventsCommand::Submit { json } => {
            let body = json_arg(json, "payload")?;
            client.post("/api/v1/events", &body).await?
        }
    };
    output::print_json(&data);
    Ok(())
}

async fn apps(client: &Client, cmd: AppsCommand) -> Result<()> {
    let path = match cmd {
        AppsCommand::List {
            devices,
            names,
            publishers,
            categories,
            versions,
            search,
            installed_from,
            installed_to,
            size_min,
            size_max,
            filters,
            load_all,
            include_archived,
            limit,
            offset,
            device_limit,
        } => Query::new()
            .opt("deviceNames", devices)
            .opt("applicationNames", names)
            .opt("publishers", publishers)
            .opt("categories", categories)
            .opt("versions", versions)
            .opt("search", search)
            .opt("installDateFrom", installed_from)
            .opt("installDateTo", installed_to)
            .opt("sizeMin", size_min)
            .opt("sizeMax", size_max)
            .filters(&filters)
            .flag("loadAll", load_all)
            .flag("includeArchived", include_archived)
            .opt("limit", limit)
            .opt("offset", offset)
            .opt("deviceLimit", device_limit)
            .path("/api/v1/applications"),
        AppsCommand::Filters { include_archived } => Query::new()
            .flag("includeArchived", include_archived)
            .path("/api/v1/applications/filters"),
        AppsCommand::Usage {
            days,
            names,
            min_hours,
            min_launches,
            filters,
            include_archived,
        } => Query::new()
            .opt("days", days)
            .opt("applicationNames", names)
            .opt("minHours", min_hours)
            .opt("minLaunches", min_launches)
            .filters(&filters)
            .flag("includeArchived", include_archived)
            .path("/api/v1/applications/usage"),
        AppsCommand::ByDevice {
            app,
            days,
            usages,
            catalogs,
            locations,
            include_archived,
        } => Query::new()
            .set("app", app)
            .opt("days", days)
            .opt("usages", usages)
            .opt("catalogs", catalogs)
            .opt("locations", locations)
            .flag("includeArchived", include_archived)
            .path("/api/v1/applications/usage/by-device"),
        AppsCommand::Distribution {
            names,
            filters,
            include_archived,
        } => Query::new()
            .set("applicationNames", names)
            .filters(&filters)
            .flag("includeArchived", include_archived)
            .path("/api/v1/applications/distribution"),
        AppsCommand::CollectionHealth {
            fresh_days,
            stale_days,
            include_archived,
        } => Query::new()
            .opt("freshDays", fresh_days)
            .opt("staleDays", stale_days)
            .flag("includeArchived", include_archived)
            .path("/api/v1/applications/collection-health"),
    };
    let data = client.get(&path).await?;
    output::print_json(&data);
    Ok(())
}

async fn admin(client: &Client, cmd: AdminCommand) -> Result<()> {
    let cmd = match cmd {
        AdminCommand::CleanupUsage { months } => {
            AdminCommand::UsageHistory(UsageHistoryCommand::Cleanup { months })
        }
        AdminCommand::ClearErrors { days } => {
            AdminCommand::Installs(InstallsCommand::ClearErrors {
                days,
                item_age_days: None,
            })
        }
        other => other,
    };
    let data = match cmd {
        AdminCommand::UsageHistory(sub) => match sub {
            UsageHistoryCommand::DateAnomalies { floor, limit } => {
                let q = Query::new().opt("floor", floor).opt("limit", limit);
                client
                    .get(&q.path("/api/v1/admin/usage-history/date-anomalies"))
                    .await?
            }
            UsageHistoryCommand::Integrity { days, sample } => {
                let q = Query::new().opt("days", days).opt("sample", sample);
                client
                    .get(&q.path("/api/v1/admin/usage-history/integrity"))
                    .await?
            }
            UsageHistoryCommand::Export { from, to, out } => {
                let q = Query::new().set("from", from).set("to", to);
                let csv = client
                    .get_text(&q.path("/api/v1/admin/usage-history/export"))
                    .await?;
                match out {
                    Some(path) => {
                        std::fs::write(&path, &csv).with_context(|| format!("writing {path}"))?;
                        let rows = csv.lines().count().saturating_sub(1);
                        eprintln!("wrote {rows} rows to {path}");
                    }
                    None => output::print_text(&csv),
                }
                return Ok(());
            }
            UsageHistoryCommand::ResetBaseline {
                before,
                confirm,
                reason,
            } => {
                let q = Query::new()
                    .set("before", before)
                    .flag("confirm", confirm)
                    .opt("reason", reason);
                client
                    .post_empty(&q.path("/api/v1/admin/usage-history/reset-baseline"))
                    .await?
            }
            UsageHistoryCommand::Cleanup { months } => {
                let q = Query::new().set("months", months);
                client
                    .delete(&q.path("/api/v1/admin/usage-history/cleanup"))
                    .await?
            }
        },
        AdminCommand::Installs(sub) => match sub {
            InstallsCommand::ClearErrors {
                days,
                item_age_days,
            } => {
                let q = Query::new()
                    .set("days", days)
                    .opt("item_age_days", item_age_days);
                client
                    .delete(&q.path("/api/v1/admin/installs/clear-errors"))
                    .await?
            }
            InstallsCommand::Reclassify { batch, limit } => {
                let q = Query::new().opt("batch", batch).opt("limit", limit);
                client
                    .post_empty(&q.path("/api/v1/admin/installs/reclassify"))
                    .await?
            }
        },
        AdminCommand::DebugDatabase => client.get("/api/v1/debug/database").await?,
        AdminCommand::CleanupUsage { .. } | AdminCommand::ClearErrors { .. } => unreachable!(),
    };
    output::print_json(&data);
    Ok(())
}

/// `reportmateutil config`: the resolved endpoint, the credential kind and the
/// source of each, with no secret material.
async fn print_config(format: OutputFormat) -> anyhow::Result<()> {
    let r = config::Resolution::resolve().await;
    let credential_kind = r.credential.as_ref().map(|c| c.kind().to_string());
    match format {
        OutputFormat::Json => {
            let v = serde_json::json!({
                "apiUrl": r.api_url,
                "apiUrlSource": r.sources.api_url,
                "credential": credential_kind,
                "credentialSource": r.sources.credential,
                "audienceSource": r.sources.audience,
                "internalSecret": r.internal_secret.is_some(),
                "notes": r.notes,
            });
            output::print_json(&v);
        }
        OutputFormat::Table => {
            println!(
                "API endpoint : {}",
                r.api_url.as_deref().unwrap_or("(none)")
            );
            println!(
                "  from       : {}",
                r.sources.api_url.as_deref().unwrap_or("-")
            );
            println!(
                "Credential   : {}",
                credential_kind.as_deref().unwrap_or("(none)")
            );
            println!(
                "  from       : {}",
                r.sources.credential.as_deref().unwrap_or("-")
            );
            if let Some(a) = &r.sources.audience {
                println!("Entra audience from : {a}");
            }
            if r.internal_secret.is_some() {
                println!("Internal secret     : set (REPORTMATE_INTERNAL_SECRET)");
            }
            for n in &r.notes {
                println!("Note: {n}");
            }
            if r.api_url.is_none() || r.credential.is_none() {
                println!();
                println!("Resolution order: environment (REPORTMATE_API_URL, REPORTMATE_TOKEN / REPORTMATE_API_KEY / REPORTMATE_PASSPHRASE),");
                println!("the ReportMate app's saved connection on this machine, the device runner's preferences,");
                println!("then the deployment's cloud sign-in (az login for an Entra audience, aws sso login for AWS).");
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{query_string, urlencode, Query};

    #[test]
    fn urlencode_leaves_unreserved_chars() {
        assert_eq!(urlencode("aZ09-_.~"), "aZ09-_.~");
    }

    #[test]
    fn urlencode_percent_encodes_reserved() {
        assert_eq!(urlencode("a b"), "a%20b");
        assert_eq!(urlencode("k=v&x"), "k%3Dv%26x");
        assert_eq!(urlencode("/full"), "%2Ffull");
    }

    #[test]
    fn urlencode_handles_multibyte_utf8() {
        // é is two UTF-8 bytes; both must be percent-encoded.
        assert_eq!(urlencode("é"), "%C3%A9");
    }

    #[test]
    fn query_string_empty_when_no_pairs() {
        assert_eq!(query_string(&[]), "");
    }

    #[test]
    fn query_string_joins_and_encodes() {
        let pairs = vec![
            ("limit".to_string(), "10".to_string()),
            ("q".to_string(), "a b".to_string()),
        ];
        assert_eq!(query_string(&pairs), "?limit=10&q=a%20b");
    }

    #[test]
    fn query_builder_skips_absent_values() {
        let q = Query::new()
            .opt("limit", None::<u32>)
            .flag("includeArchived", false)
            .opt("days", Some(7))
            .flag("summary", true);
        assert_eq!(q.path("/x"), "/x?days=7&summary=true");
    }

    #[test]
    fn query_builder_appends_raw_params() {
        let q = Query::new()
            .set("a", 1)
            .params(&["b=two".to_string(), "c=3".to_string()])
            .unwrap();
        assert_eq!(q.path("/x"), "/x?a=1&b=two&c=3");
        assert!(Query::new().params(&["novalue".to_string()]).is_err());
    }
}
