use std::time::Duration;

use anyhow::{bail, Result};
use serde_json::Value;

use crate::config::{Config, Credential};

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
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .user_agent(format!("reportmate-cli/{}", crate::cli::VERSION))
            .build()?;
        Ok(Client { http, cfg })
    }

    fn authed(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.cfg.credential {
            Credential::ApiKey(k) => req.header("X-API-Key", k),
            Credential::Passphrase(p) => req.header("X-Client-Passphrase", p),
        }
    }

    async fn send(&self, req: reqwest::RequestBuilder, method: &str, path: &str) -> Result<Value> {
        let resp = self
            .authed(req)
            .header("Accept", "application/json")
            .send()
            .await?;

        let status = resp.status();
        let body = resp.text().await?;
        if !status.is_success() {
            bail!("{} {} -> {}: {}", method, path, status, body);
        }
        if body.is_empty() {
            return Ok(Value::Null);
        }
        Ok(serde_json::from_str(&body)?)
    }

    pub async fn get(&self, path: &str) -> Result<Value> {
        let url = format!("{}{}", self.cfg.api_url, path);
        self.send(self.http.get(&url), "GET", path).await
    }

    pub async fn post(&self, path: &str, body: &Value) -> Result<Value> {
        let url = format!("{}{}", self.cfg.api_url, path);
        self.send(self.http.post(&url).json(body), "POST", path).await
    }

    pub async fn delete(&self, path: &str) -> Result<Value> {
        let url = format!("{}{}", self.cfg.api_url, path);
        self.send(self.http.delete(&url), "DELETE", path).await
    }
}
