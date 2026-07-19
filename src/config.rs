use anyhow::{bail, Result};

/// Runtime configuration, resolved from the environment.
///
/// Credentials, in order of preference:
/// - `REPORTMATE_API_KEY` — a scoped per-client key (`rm_<id>_<secret>`),
///   sent as `X-API-Key`. Preferred: revocable and scope-limited.
/// - `REPORTMATE_PASSPHRASE` — the legacy shared client passphrase, sent as
///   `X-Client-Passphrase`.
///
/// A future revision can add a config file (`~/.config/reportmate/config.toml`)
/// and named profiles for multiple instances.
pub struct Config {
    pub api_url: String,
    pub credential: Credential,
}

pub enum Credential {
    ApiKey(String),
    Passphrase(String),
}

impl Config {
    pub fn load() -> Result<Config> {
        let api_url = match std::env::var("REPORTMATE_API_URL") {
            Ok(v) if !v.trim().is_empty() => v,
            _ => bail!("REPORTMATE_API_URL not set (e.g. https://api.reportmate.app)"),
        };

        let credential = if let Ok(key) = std::env::var("REPORTMATE_API_KEY") {
            Credential::ApiKey(key)
        } else if let Ok(pp) = std::env::var("REPORTMATE_PASSPHRASE") {
            Credential::Passphrase(pp)
        } else {
            bail!("no credential: set REPORTMATE_API_KEY (preferred) or REPORTMATE_PASSPHRASE");
        };

        Ok(Config {
            api_url: api_url.trim_end_matches('/').to_string(),
            credential,
        })
    }
}
