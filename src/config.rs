use anyhow::{bail, Result};

/// Runtime configuration, resolved from the environment.
///
/// Credentials, in order of preference:
/// - `REPORTMATE_API_KEY` — a scoped per-client key (`rm_<id>_<secret>`),
///   sent as `X-API-Key`. Preferred: revocable and scope-limited.
/// - `REPORTMATE_TOKEN` — an OIDC bearer token from the org's identity
///   provider, sent as `Authorization: Bearer`.
/// - `REPORTMATE_PASSPHRASE` — the legacy shared client passphrase, sent as
///   `X-Client-Passphrase`.
///
/// `REPORTMATE_INTERNAL_SECRET` is optional and only needed for internal-only
/// calls (`settings set`, `settings discover`), sent as `X-Internal-Secret`.
///
/// A future revision can add a config file (`~/.config/reportmate/config.toml`)
/// and named profiles for multiple instances.
pub struct Config {
    pub api_url: String,
    pub credential: Credential,
    pub internal_secret: Option<String>,
}

pub enum Credential {
    ApiKey(String),
    Bearer(String),
    Passphrase(String),
}

fn env_nonempty(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

impl Config {
    pub fn load() -> Result<Config> {
        let api_url = match env_nonempty("REPORTMATE_API_URL") {
            Some(v) => v,
            None => bail!("REPORTMATE_API_URL not set (e.g. https://api.reportmate.app)"),
        };

        let credential = if let Some(key) = env_nonempty("REPORTMATE_API_KEY") {
            Credential::ApiKey(key)
        } else if let Some(token) = env_nonempty("REPORTMATE_TOKEN") {
            Credential::Bearer(token)
        } else if let Some(pp) = env_nonempty("REPORTMATE_PASSPHRASE") {
            Credential::Passphrase(pp)
        } else {
            bail!("no credential: set REPORTMATE_API_KEY (preferred), REPORTMATE_TOKEN, or REPORTMATE_PASSPHRASE");
        };

        Ok(Config {
            api_url: api_url.trim_end_matches('/').to_string(),
            credential,
            internal_secret: env_nonempty("REPORTMATE_INTERNAL_SECRET"),
        })
    }
}
