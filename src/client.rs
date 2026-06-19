use anyhow::{bail, Result};
use serde_json::Value;

use crate::config::Config;

/// Thin async client over the ReportMate REST API (`/api/v1/*`).
///
/// Responses are returned as untyped `serde_json::Value` for now. Once the
/// CLI tracks the API's published OpenAPI spec, this can be replaced with a
/// generated, typed client (e.g. via `progenitor`).
pub struct Client {
    http: reqwest::Client,
    cfg: Config,
}

impl Client {
    pub fn new(cfg: Config) -> Result<Client> {
        Ok(Client {
            http: reqwest::Client::new(),
            cfg,
        })
    }

    pub async fn get(&self, path: &str) -> Result<Value> {
        let url = format!("{}{}", self.cfg.api_url, path);
        let resp = self
            .http
            .get(&url)
            .header("X-Client-Passphrase", &self.cfg.passphrase)
            .header("Accept", "application/json")
            .send()
            .await?;

        let status = resp.status();
        let body = resp.text().await?;
        if !status.is_success() {
            bail!("GET {} -> {}: {}", path, status, body);
        }
        Ok(serde_json::from_str(&body)?)
    }
}
