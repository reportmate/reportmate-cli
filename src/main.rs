mod cli;
mod client;
mod config;
mod output;

use anyhow::{bail, Result};
use clap::Parser;
use serde_json::json;

use cli::{AdminCommand, ApiKeysCommand, Cli, Command, OutputFormat, SettingsCommand};

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

/// Restore the default SIGPIPE disposition.
///
/// Rust ignores SIGPIPE by default, so a consumer that closes stdout early
/// (e.g. `reportmate devices | head`) turns the next write into a panic
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
    let cfg = config::Config::load()?;
    let client = client::Client::new(cfg)?;

    match args.command {
        Command::Devices {
            limit,
            offset,
            include_archived,
        } => {
            let mut q: Vec<(String, String)> = Vec::new();
            if let Some(l) = limit {
                q.push(("limit".into(), l.to_string()));
            }
            if let Some(o) = offset {
                q.push(("offset".into(), o.to_string()));
            }
            if include_archived {
                q.push(("includeArchived".into(), "true".into()));
            }
            let data = client
                .get(&format!("/api/v1/devices{}", query_string(&q)))
                .await?;
            match args.output {
                OutputFormat::Json => output::print_json(&data),
                OutputFormat::Table => output::print_devices_table(&data),
            }
        }
        Command::Device { serial, module } => {
            let path = match module {
                Some(m) => format!("/api/v1/device/{serial}/modules/{m}"),
                None => format!("/api/v1/device/{serial}"),
            };
            let data = client.get(&path).await?;
            // Device detail is deeply nested; always JSON.
            output::print_json(&data);
        }
        Command::Module { name, params } => {
            let mut q: Vec<(String, String)> = Vec::new();
            for p in &params {
                match p.split_once('=') {
                    Some((k, v)) => q.push((k.to_string(), v.to_string())),
                    None => bail!("--param must be key=value, got: {p}"),
                }
            }
            let name = name.trim_matches('/');
            let data = client
                .get(&format!("/api/v1/{name}{}", query_string(&q)))
                .await?;
            output::print_json(&data);
        }
        Command::Events { limit } => {
            let mut q: Vec<(String, String)> = Vec::new();
            if let Some(l) = limit {
                q.push(("limit".into(), l.to_string()));
            }
            let data = client
                .get(&format!("/api/v1/events{}", query_string(&q)))
                .await?;
            match args.output {
                OutputFormat::Json => output::print_json(&data),
                OutputFormat::Table => output::print_events_table(&data),
            }
        }
        Command::Health { ready } => {
            let path = if ready {
                "/api/v1/health/ready"
            } else {
                "/api/v1/health/live"
            };
            let data = client.get(path).await?;
            output::print_json(&data);
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
        Command::Archive { serial } => {
            let data = client
                .patch(&format!("/api/v1/device/{serial}/archive"))
                .await?;
            output::print_json(&data);
        }
        Command::Unarchive { serial } => {
            let data = client
                .patch(&format!("/api/v1/device/{serial}/unarchive"))
                .await?;
            output::print_json(&data);
        }
        Command::Delete { serial, confirm } => {
            if !confirm {
                bail!("refusing to delete {serial} without --confirm");
            }
            let data = client
                .delete(&format!("/api/v1/device/{serial}?confirm=true"))
                .await?;
            output::print_json(&data);
        }
        Command::Admin(cmd) => match cmd {
            AdminCommand::CleanupUsage { months } => {
                let data = client
                    .delete(&format!(
                        "/api/v1/admin/usage-history/cleanup?months={months}"
                    ))
                    .await?;
                output::print_json(&data);
            }
            AdminCommand::ClearErrors { days } => {
                let data = client
                    .delete(&format!("/api/v1/admin/installs/clear-errors?days={days}"))
                    .await?;
                output::print_json(&data);
            }
        },
        Command::Settings(cmd) => match cmd {
            SettingsCommand::Get => {
                let data = client.get("/api/v1/settings").await?;
                output::print_json(&data);
            }
            SettingsCommand::Set { json } => {
                let raw = if let Some(path) = json.strip_prefix('@') {
                    std::fs::read_to_string(path)?
                } else {
                    json
                };
                let body: serde_json::Value = serde_json::from_str(&raw)
                    .map_err(|e| anyhow::anyhow!("settings must be valid JSON: {e}"))?;
                let data = client.put("/api/v1/settings", &body).await?;
                output::print_json(&data);
            }
        },
        Command::Raw { path } => {
            if !path.starts_with('/') {
                bail!("path must start with /, e.g. /api/v1/dashboard");
            }
            let data = client.get(&path).await?;
            output::print_json(&data);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{query_string, urlencode};

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
}
