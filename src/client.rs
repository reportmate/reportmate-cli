use std::time::Duration;

use anyhow::{bail, Result};
use reqwest::Method;
use serde_json::Value;

use crate::config::{Config, Credential};

/// A response body, kept as text when the endpoint does not speak JSON
/// (CSV exports, Prometheus metrics).
pub enum Body {
    Json(Value),
    Text(String),
}

impl Body {
    /// The JSON value, or an error if the endpoint returned something else.
    pub fn into_json(self, path: &str) -> Result<Value> {
        match self {
            Body::Json(v) => Ok(v),
            Body::Text(t) => bail!(
                "{path} returned a non-JSON body: {}",
                t.chars().take(200).collect::<String>()
            ),
        }
    }
}

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
            .timeout(Duration::from_secs(600))
            .user_agent(format!("reportmate-cli/{}", crate::cli::VERSION))
            .build()?;
        Ok(Client { http, cfg })
    }

    fn authed(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        let req = match &self.cfg.credential {
            Credential::ApiKey(k) => req.header("X-API-Key", k),
            Credential::Bearer(t) => req.header("Authorization", format!("Bearer {t}")),
            Credential::Passphrase(p) => req.header("X-Client-Passphrase", p),
        };
        // Internal secret authorizes internal-only writes (e.g. settings);
        // harmless on other requests.
        match &self.cfg.internal_secret {
            Some(s) => req.header("X-Internal-Secret", s),
            None => req,
        }
    }

    /// Send a request and return the body, parsed as JSON when the server
    /// says it is JSON (or when it parses anyway), otherwise as text.
    pub async fn request(
        &self,
        method: Method,
        path: &str,
        body: Option<&Value>,
        headers: &[(&str, &str)],
    ) -> Result<Body> {
        let url = format!("{}{}", self.cfg.api_url, path);
        let mut req = self.http.request(method.clone(), &url);
        if let Some(b) = body {
            req = req.json(b);
        }
        for (k, v) in headers {
            req = req.header(*k, *v);
        }
        let resp = self
            .authed(req)
            .header("Accept", "application/json, text/plain;q=0.9, */*;q=0.8")
            .send()
            .await?;

        let status = resp.status();
        let content_type = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        let text = resp.text().await?;
        if !status.is_success() {
            bail!("{} {} -> {}: {}", method, path, status, text);
        }
        if text.is_empty() {
            return Ok(Body::Json(Value::Null));
        }
        if content_type.contains("json") {
            return Ok(Body::Json(serde_json::from_str(&text)?));
        }
        match serde_json::from_str::<Value>(&text) {
            Ok(v) if !content_type.starts_with("text/") => Ok(Body::Json(v)),
            _ => Ok(Body::Text(text)),
        }
    }

    async fn json(&self, method: Method, path: &str, body: Option<&Value>) -> Result<Value> {
        self.request(method, path, body, &[]).await?.into_json(path)
    }

    pub async fn get(&self, path: &str) -> Result<Value> {
        self.json(Method::GET, path, None).await
    }

    /// GET a text endpoint (CSV export, metrics). A JSON body is returned
    /// pretty-printed so the caller never has to branch.
    pub async fn get_text(&self, path: &str) -> Result<String> {
        match self.request(Method::GET, path, None, &[]).await? {
            Body::Text(t) => Ok(t),
            Body::Json(v) => Ok(serde_json::to_string_pretty(&v)?),
        }
    }

    pub async fn post(&self, path: &str, body: &Value) -> Result<Value> {
        self.json(Method::POST, path, Some(body)).await
    }

    pub async fn post_empty(&self, path: &str) -> Result<Value> {
        self.json(Method::POST, path, None).await
    }

    pub async fn delete(&self, path: &str) -> Result<Value> {
        self.json(Method::DELETE, path, None).await
    }

    pub async fn patch(&self, path: &str) -> Result<Value> {
        self.json(Method::PATCH, path, None).await
    }

    pub async fn put(&self, path: &str, body: &Value, headers: &[(&str, &str)]) -> Result<Value> {
        self.request(Method::PUT, path, Some(body), headers)
            .await?
            .into_json(path)
    }
}
