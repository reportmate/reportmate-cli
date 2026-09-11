use std::path::PathBuf;
use std::process::Command;

use anyhow::{bail, Result};

/// Runtime configuration: the API endpoint and the credential to present.
///
/// Nothing has to be exported by hand on an admin machine. Each value is
/// resolved from the first source that has it, in this order:
///
/// 1. Environment: `REPORTMATE_API_URL`, then `REPORTMATE_API_KEY` (`X-API-Key`,
///    preferred), `REPORTMATE_TOKEN` (an OIDC bearer JWT) or
///    `REPORTMATE_PASSPHRASE` (`X-Client-Passphrase`), and
///    `REPORTMATE_OIDC_AUDIENCE` for Entra sign-in.
/// 2. The ReportMate admin app's saved connection on this machine: the
///    non-secret part from `connection.json` (macOS:
///    `~/Library/Application Support/ReportMate/`, Windows:
///    `%ProgramData%\ReportMate\`), the secret from the macOS Keychain
///    (service `com.github.reportmate.mac`, the app's own items).
/// 3. The device runner's own preferences (macOS `com.github.reportmate`
///    domain, Windows `HKLM\SOFTWARE\ReportMate`): the API URL and the
///    shared passphrase. The runner's API key is ingest-only and never used.
/// 4. Cloud sign-in for the deployment: an Entra audience (`api://…` or an
///    app id) is exchanged for a token through `az account get-access-token`,
///    the way the Mac app's Entra sign-in works; an AWS deployment reads the
///    client passphrase from Secrets Manager (`REPORTMATE_AWS_SECRET_ID`,
///    default `reportmate/client-passphrase`) with the caller's AWS session.
///
/// `REPORTMATE_NO_DISCOVERY=1` confines resolution to the environment.
///
/// `REPORTMATE_INTERNAL_SECRET` is optional and only needed for internal-only
/// writes (currently `settings set`), sent as `X-Internal-Secret`.
pub struct Config {
    pub api_url: String,
    pub credential: Credential,
    pub internal_secret: Option<String>,
    pub sources: Sources,
}

pub enum Credential {
    ApiKey(String),
    Passphrase(String),
    Bearer(String),
}

impl Credential {
    pub fn kind(&self) -> &'static str {
        match self {
            Credential::ApiKey(_) => "api key (X-API-Key)",
            Credential::Passphrase(_) => "passphrase (X-Client-Passphrase)",
            Credential::Bearer(_) => "bearer token (Authorization)",
        }
    }
}

/// Where each resolved value came from, for `reportmateutil config`.
#[derive(Default, Clone)]
pub struct Sources {
    pub api_url: Option<String>,
    pub credential: Option<String>,
    pub audience: Option<String>,
}

/// The admin app's saved connection, minus secrets.
#[derive(Default, Clone, Debug, PartialEq)]
pub struct AppConnection {
    pub api_url: Option<String>,
    pub auth_method: Option<String>,
    pub oidc_audience: Option<String>,
    pub cloud: Option<String>,
}

impl Config {
    pub fn load() -> Result<Config> {
        let r = Resolution::resolve();
        let api_url = match r.api_url {
            Some(u) => u,
            None => bail!(
                "no API endpoint: set REPORTMATE_API_URL, or connect the ReportMate app on this machine (Settings → Connection)"
            ),
        };
        let credential = match r.credential {
            Some(c) => c,
            None => bail!(
                "no credential: set REPORTMATE_API_KEY (preferred), REPORTMATE_PASSPHRASE or REPORTMATE_TOKEN, connect the ReportMate app on this machine, or sign in to the deployment's cloud (az login / aws sso login){}",
                r.notes.iter().map(|n| format!("\n  {n}")).collect::<String>()
            ),
        };
        Ok(Config {
            api_url,
            credential,
            internal_secret: r.internal_secret,
            sources: r.sources,
        })
    }
}

/// Everything the resolver found, including why a step was skipped.
pub struct Resolution {
    pub api_url: Option<String>,
    pub credential: Option<Credential>,
    pub internal_secret: Option<String>,
    pub sources: Sources,
    pub notes: Vec<String>,
}

impl Resolution {
    pub fn resolve() -> Resolution {
        let mut sources = Sources::default();
        let mut notes = Vec::new();
        // REPORTMATE_NO_DISCOVERY=1 confines resolution to the environment: for CI,
        // tests and scripts that must not pick up whatever this machine has.
        let discover = env_value("REPORTMATE_NO_DISCOVERY").map(|v| !matches!(v.as_str(), "1" | "true" | "yes")).unwrap_or(true);
        let app = if discover { read_app_connection() } else { AppConnection::default() };
        let runner = if discover { read_runner_prefs() } else { RunnerPrefs { api_url: None, passphrase: None } };

        // Endpoint.
        let mut api_url = env_value("REPORTMATE_API_URL").map(|u| (u, "REPORTMATE_API_URL".to_string()));
        if api_url.is_none() {
            if let Some(u) = app.api_url.clone() {
                api_url = Some((u, "ReportMate app connection".to_string()));
            }
        }
        if api_url.is_none() {
            if let Some(u) = runner.api_url.clone() {
                api_url = Some((u, "device runner preferences".to_string()));
            }
        }
        let api_url = api_url.map(|(u, s)| {
            sources.api_url = Some(s);
            u.trim().trim_end_matches('/').to_string()
        });

        // Audience for Entra sign-in: env, then the app's saved one.
        let mut audience = env_value("REPORTMATE_OIDC_AUDIENCE").map(|a| (a, "REPORTMATE_OIDC_AUDIENCE".to_string()));
        if audience.is_none() {
            if let Some(a) = app.oidc_audience.clone() {
                audience = Some((a, "ReportMate app connection".to_string()));
            }
        }
        if let Some((_, s)) = &audience {
            sources.audience = Some(s.clone());
        }

        // Credential, first source wins.
        let mut credential: Option<(Credential, String)> = None;
        if let Some(k) = env_value("REPORTMATE_API_KEY") {
            credential = Some((Credential::ApiKey(k), "REPORTMATE_API_KEY".into()));
        } else if let Some(t) = env_value("REPORTMATE_TOKEN") {
            credential = Some((Credential::Bearer(t), "REPORTMATE_TOKEN".into()));
        } else if let Some(p) = env_value("REPORTMATE_PASSPHRASE") {
            credential = Some((Credential::Passphrase(p), "REPORTMATE_PASSPHRASE".into()));
        }

        // The app's saved credential, by the method the app itself uses.
        if credential.is_none() {
            match app.auth_method.as_deref() {
                Some("entraBearer") => {
                    if let Some((aud, _)) = &audience {
                        match az_token(aud) {
                            Ok(t) => credential = Some((Credential::Bearer(t), "Entra sign-in via az (app connection)".into())),
                            Err(e) => notes.push(format!("Entra sign-in: {e}")),
                        }
                    }
                }
                Some("apiKey") => {
                    if let Some(k) = read_app_secret("ApiKey") {
                        credential = Some((Credential::ApiKey(k), "ReportMate app connection (API key)".into()));
                    }
                }
                Some("passphrase") => {
                    if let Some(p) = read_app_secret("Passphrase") {
                        credential = Some((Credential::Passphrase(p), "ReportMate app connection (passphrase)".into()));
                    }
                }
                _ => {
                    // No method recorded: take whatever secret the app holds.
                    if let Some(k) = read_app_secret("ApiKey") {
                        credential = Some((Credential::ApiKey(k), "ReportMate app connection (API key)".into()));
                    } else if let Some(p) = read_app_secret("Passphrase") {
                        credential = Some((Credential::Passphrase(p), "ReportMate app connection (passphrase)".into()));
                    }
                }
            }
        }

        // The runner's shared passphrase (its API key is ingest-only).
        if credential.is_none() {
            if let Some(p) = runner.passphrase.clone() {
                credential = Some((Credential::Passphrase(p), "device runner preferences (passphrase)".into()));
            }
        }

        // Cloud sign-in for the deployment.
        if credential.is_none() && discover {
            if let Some((aud, _)) = &audience {
                match az_token(aud) {
                    Ok(t) => credential = Some((Credential::Bearer(t), "Entra sign-in via az".into())),
                    Err(e) => notes.push(format!("Entra sign-in: {e}")),
                }
            }
        }
        if credential.is_none() && discover {
            let cloud = app.cloud.clone().or_else(|| env_value("REPORTMATE_CLOUD")).or_else(|| api_url.as_deref().and_then(detect_cloud));
            if cloud.as_deref() == Some("aws") {
                let secret_id = env_value("REPORTMATE_AWS_SECRET_ID").unwrap_or_else(|| "reportmate/client-passphrase".to_string());
                match aws_secret(&secret_id) {
                    Ok(p) => credential = Some((Credential::Passphrase(p), format!("AWS Secrets Manager {secret_id}"))),
                    Err(e) => notes.push(format!("AWS Secrets Manager {secret_id}: {e}")),
                }
            }
        }

        let credential = credential.map(|(c, s)| {
            sources.credential = Some(s);
            c
        });

        Resolution {
            api_url,
            credential,
            internal_secret: env_value("REPORTMATE_INTERNAL_SECRET"),
            sources,
            notes,
        }
    }
}

fn env_value(name: &str) -> Option<String> {
    std::env::var(name).ok().map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

/// `aws` for an AWS-hosted API, `azure` for an Azure one, by the host alone.
pub fn detect_cloud(api_url: &str) -> Option<String> {
    let host = api_url.split("//").nth(1).unwrap_or(api_url).split('/').next().unwrap_or("").to_ascii_lowercase();
    if host.ends_with(".amazonaws.com") || host.contains(".execute-api.") || host.ends_with(".awsapprunner.com") {
        Some("aws".into())
    } else if host.ends_with(".azurecontainerapps.io") || host.ends_with(".azurewebsites.net") || host.ends_with(".azure-api.net") {
        Some("azure".into())
    } else {
        None
    }
}

/// The app's shared `connection.json` (non-secret), plus the macOS Keychain's
/// non-secret items when the file is absent.
fn read_app_connection() -> AppConnection {
    let mut conn = connection_file_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| parse_connection_json(&s))
        .unwrap_or_default();
    if cfg!(target_os = "macos") {
        if conn.api_url.is_none() {
            conn.api_url = keychain_item("ApiBaseUrl");
        }
        if conn.auth_method.is_none() {
            conn.auth_method = keychain_item("AuthMethod");
        }
        if conn.oidc_audience.is_none() {
            conn.oidc_audience = keychain_item("OidcAudience");
        }
    }
    conn
}

pub fn parse_connection_json(text: &str) -> Option<AppConnection> {
    let v: serde_json::Value = serde_json::from_str(text).ok()?;
    let s = |k: &str| v.get(k).and_then(|x| x.as_str()).map(|x| x.trim().to_string()).filter(|x| !x.is_empty());
    Some(AppConnection {
        api_url: s("apiUrl").or_else(|| s("apiBaseUrl")),
        auth_method: s("authMethod"),
        oidc_audience: s("oidcAudience"),
        cloud: s("cloud"),
    })
}

fn connection_file_path() -> Option<PathBuf> {
    if cfg!(target_os = "macos") {
        let home = std::env::var_os("HOME")?;
        Some(PathBuf::from(home).join("Library/Application Support/ReportMate/connection.json"))
    } else if cfg!(target_os = "windows") {
        let base = std::env::var_os("ProgramData")?;
        Some(PathBuf::from(base).join("ReportMate").join("connection.json"))
    } else {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
        Some(base.join("reportmate").join("connection.json"))
    }
}

/// A secret the admin app saved: the macOS Keychain item of the app's service.
fn read_app_secret(account: &str) -> Option<String> {
    if cfg!(target_os = "macos") {
        keychain_item(account)
    } else {
        None
    }
}

fn keychain_item(account: &str) -> Option<String> {
    if !cfg!(target_os = "macos") {
        return None;
    }
    run_stdout("security", &["find-generic-password", "-s", "com.github.reportmate.mac", "-a", account, "-w"])
}

/// The device runner's endpoint and shared passphrase on this machine.
struct RunnerPrefs {
    api_url: Option<String>,
    passphrase: Option<String>,
}

fn read_runner_prefs() -> RunnerPrefs {
    if cfg!(target_os = "macos") {
        RunnerPrefs {
            api_url: run_stdout("defaults", &["read", "com.github.reportmate", "ApiUrl"]),
            passphrase: run_stdout("defaults", &["read", "com.github.reportmate", "Passphrase"]),
        }
    } else if cfg!(target_os = "windows") {
        RunnerPrefs {
            api_url: registry_value(r"HKLM\SOFTWARE\ReportMate", "ApiUrl"),
            passphrase: registry_value(r"HKLM\SOFTWARE\ReportMate", "Passphrase"),
        }
    } else {
        RunnerPrefs { api_url: None, passphrase: None }
    }
}

fn registry_value(key: &str, name: &str) -> Option<String> {
    let out = run_stdout("reg", &["query", key, "/v", name])?;
    parse_reg_query(&out, name)
}

/// The value from a `reg query … /v NAME` listing: `NAME    REG_SZ    value`.
pub fn parse_reg_query(out: &str, name: &str) -> Option<String> {
    for line in out.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix(name) {
            let mut parts = rest.split_whitespace();
            let kind = parts.next()?;
            if kind.starts_with("REG_") {
                let value: Vec<&str> = parts.collect();
                let v = value.join(" ");
                if !v.is_empty() {
                    return Some(v);
                }
            }
        }
    }
    None
}

/// A delegated token for the API's Entra audience from the caller's `az login`.
fn az_token(audience: &str) -> Result<String> {
    let az = if cfg!(target_os = "windows") { "az.cmd" } else { "az" };
    match run_stdout(az, &["account", "get-access-token", "--resource", audience, "--query", "accessToken", "-o", "tsv"]) {
        Some(t) => Ok(t),
        None => bail!("az account get-access-token --resource {audience} returned nothing (run az login)"),
    }
}

/// The plain secret string from AWS Secrets Manager with the caller's session.
fn aws_secret(secret_id: &str) -> Result<String> {
    match run_stdout("aws", &["secretsmanager", "get-secret-value", "--secret-id", secret_id, "--query", "SecretString", "--output", "text"]) {
        Some(s) if s != "None" => Ok(s),
        _ => bail!("aws secretsmanager get-secret-value --secret-id {secret_id} returned nothing (run aws sso login)"),
    }
}

/// Trimmed stdout of a command that exited 0 with output, else None.
fn run_stdout(program: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(program).args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_cloud_from_host() {
        assert_eq!(detect_cloud("https://api.prod.azurecontainerapps.io"), Some("azure".into()));
        assert_eq!(detect_cloud("https://abc.execute-api.us-west-2.amazonaws.com/v1"), Some("aws".into()));
        assert_eq!(detect_cloud("https://api.example.org"), None);
    }

    #[test]
    fn parses_connection_json() {
        let c = parse_connection_json(r#"{"apiUrl":"https://api.example.org/","authMethod":"entraBearer","oidcAudience":"api://00000000-0000-0000-0000-000000000000","cloud":"azure"}"#).unwrap();
        assert_eq!(c.api_url.as_deref(), Some("https://api.example.org/"));
        assert_eq!(c.auth_method.as_deref(), Some("entraBearer"));
        assert_eq!(c.cloud.as_deref(), Some("azure"));
        assert!(parse_connection_json("not json").is_none());
    }

    #[test]
    fn parses_reg_query_output() {
        let out = "\r\nHKEY_LOCAL_MACHINE\\SOFTWARE\\ReportMate\r\n    ApiUrl    REG_SZ    https://api.example.org\r\n\r\n";
        assert_eq!(parse_reg_query(out, "ApiUrl").as_deref(), Some("https://api.example.org"));
        assert_eq!(parse_reg_query(out, "Passphrase"), None);
    }
}
