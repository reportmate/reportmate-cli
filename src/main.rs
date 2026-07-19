mod cli;
mod client;
mod config;
mod output;

use anyhow::{bail, Result};
use clap::Parser;
use serde_json::json;

use cli::{ApiKeysCommand, Cli, Command, OutputFormat};

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

#[tokio::main]
async fn main() -> Result<()> {
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
