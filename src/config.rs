use anyhow::{Context, Result};

/// Runtime configuration, resolved from the environment.
///
/// Auth uses the same `X-Client-Passphrase` header the device agents use.
/// A future revision can add a config file (`~/.config/reportmate/config.toml`)
/// and named profiles for multiple instances.
pub struct Config {
    pub api_url: String,
    pub passphrase: String,
}

impl Config {
    pub fn load() -> Result<Config> {
        let api_url = std::env::var("REPORTMATE_API_URL")
            .context("REPORTMATE_API_URL not set (e.g. https://api.reportmate.app)")?;
        let passphrase = std::env::var("REPORTMATE_PASSPHRASE")
            .context("REPORTMATE_PASSPHRASE not set")?;
        Ok(Config {
            api_url: api_url.trim_end_matches('/').to_string(),
            passphrase,
        })
    }
}
