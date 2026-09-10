//! Contract tests: what the CLI puts on the wire, and whether that covers the
//! API.
//!
//! A tiny HTTP/1.1 server records every request the binary makes; each
//! command in [`ROUTES`] is run against it and its method, path and query
//! string are compared verbatim. The same table carries the OpenAPI path
//! template each command implements, and [`every_api_operation_has_a_command`]
//! checks that table against `tests/openapi.json`, exported from the API
//! repository (see `.github/workflows/openapi-sync.yml`). A route added to the
//! API therefore fails this suite until the CLI learns it.

use std::collections::BTreeSet;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Command, Output};
use std::sync::{Arc, Mutex};
use std::thread;

#[derive(Clone, Debug)]
struct Recorded {
    method: String,
    /// Path plus query string, exactly as sent.
    target: String,
    /// Lower-cased header names.
    headers: Vec<(String, String)>,
    body: String,
}

impl Recorded {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }
}

/// A recording stand-in for the API. Answers JSON everywhere except the two
/// text endpoints, and 403 under `/api/v1/forbidden`.
struct MockApi {
    url: String,
    log: Arc<Mutex<Vec<Recorded>>>,
}

impl MockApi {
    fn start() -> MockApi {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let url = format!("http://{}", listener.local_addr().unwrap());
        let log = Arc::new(Mutex::new(Vec::new()));
        let sink = log.clone();
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { break };
                let sink = sink.clone();
                thread::spawn(move || serve(stream, sink));
            }
        });
        MockApi { url, log }
    }

    fn last(&self) -> Recorded {
        self.log
            .lock()
            .unwrap()
            .last()
            .cloned()
            .expect("the CLI made no request")
    }
}

fn serve(mut stream: TcpStream, log: Arc<Mutex<Vec<Recorded>>>) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).unwrap_or(0) == 0 {
        return;
    }
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let target = parts.next().unwrap_or("").to_string();

    let mut headers = Vec::new();
    let mut content_length = 0usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 {
            break;
        }
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        if let Some((k, v)) = line.split_once(':') {
            let k = k.trim().to_ascii_lowercase();
            let v = v.trim().to_string();
            if k == "content-length" {
                content_length = v.parse().unwrap_or(0);
            }
            headers.push((k, v));
        }
    }
    let mut body = vec![0u8; content_length];
    if content_length > 0 {
        reader.read_exact(&mut body).expect("request body");
    }

    let path = target.split('?').next().unwrap_or("").to_string();
    let (status, content_type, payload) = match path.as_str() {
        "/api/v1/metrics" => (
            "200 OK",
            "text/plain; version=0.0.4",
            "rm_up 1\n".to_string(),
        ),
        "/api/v1/admin/usage-history/export" => {
            ("200 OK", "text/csv", "a,b\n1,2\n3,4\n".to_string())
        }
        p if p.starts_with("/api/v1/forbidden") => (
            "403 Forbidden",
            "application/json",
            r#"{"error":"Forbidden","detail":"missing scope"}"#.to_string(),
        ),
        p => (
            "200 OK",
            "application/json",
            format!(r#"{{"ok":true,"path":"{p}"}}"#),
        ),
    };

    // Record before answering, so the entry exists by the time the CLI exits.
    log.lock().unwrap().push(Recorded {
        method,
        target,
        headers,
        body: String::from_utf8_lossy(&body).into_owned(),
    });

    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
        payload.len()
    );
    stream.write_all(response.as_bytes()).ok();
    stream.flush().ok();
}

/// Runs the built binary against the mock with an API key, and nothing
/// else ReportMate-related in its environment.
fn reportmate(api: &MockApi, args: &[&str]) -> Output {
    reportmate_env(api, args, &[("REPORTMATE_API_KEY", "rm_test_key")])
}

fn reportmate_env(api: &MockApi, args: &[&str], env: &[(&str, &str)]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_reportmate"));
    for key in [
        "REPORTMATE_API_URL",
        "REPORTMATE_API_KEY",
        "REPORTMATE_TOKEN",
        "REPORTMATE_PASSPHRASE",
        "REPORTMATE_INTERNAL_SECRET",
    ] {
        cmd.env_remove(key);
    }
    cmd.env("REPORTMATE_API_URL", &api.url);
    for (k, v) in env {
        cmd.env(k, v);
    }
    cmd.args(args).output().expect("run reportmate")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// One CLI invocation and the request it must produce.
struct Route {
    args: &'static [&'static str],
    method: &'static str,
    /// Path and query, byte for byte.
    target: &'static str,
    /// The OpenAPI path this command implements.
    template: &'static str,
}

macro_rules! route {
    ([$($arg:expr),* $(,)?] => $method:literal $target:literal as $template:literal) => {
        Route { args: &[$($arg),*], method: $method, target: $target, template: $template }
    };
}

/// Every command, with the exact wire request it stands for. Keep this in
/// step with `src/cli.rs`; the parity test keeps it in step with the API.
const ROUTES: &[Route] = &[
    // Devices
    route!(["devices", "--limit", "5", "--offset", "10", "--include-archived"]
        => "GET" "/api/v1/devices?limit=5&offset=10&includeArchived=true" as "/api/v1/devices"),
    route!(["device", "SER1"] => "GET" "/api/v1/device/SER1" as "/api/v1/device/{serial_number}"),
    route!(["device", "SER1", "--module", "installs"]
        => "GET" "/api/v1/device/SER1/modules/installs" as "/api/v1/device/{serial_number}/modules/{module_name}"),
    route!(["device", "SER1", "module", "hardware"]
        => "GET" "/api/v1/device/SER1/modules/hardware" as "/api/v1/device/{serial_number}/modules/{module_name}"),
    route!(["device", "SER1", "info"] => "GET" "/api/v1/device/SER1/info" as "/api/v1/device/{serial_number}/info"),
    route!(["device", "SER1", "events", "--limit", "3", "--type", "error"]
        => "GET" "/api/v1/device/SER1/events?limit=3&type=error" as "/api/v1/device/{serial_number}/events"),
    route!(["device", "SER1", "installs-log"]
        => "GET" "/api/v1/device/SER1/installs/log" as "/api/v1/device/{serial_number}/installs/log"),
    route!(["device", "SER1", "log", "munki"]
        => "GET" "/api/v1/device/SER1/logs/munki" as "/api/v1/device/{serial_number}/logs/{tool}"),
    route!(["device", "SER1", "usage", "--days", "30", "--app", "Adobe Photoshop"]
        => "GET" "/api/v1/device/SER1/applications/usage/history?days=30&appName=Adobe%20Photoshop"
        as "/api/v1/device/{serial_number}/applications/usage/history"),
    route!(["device", "SER1", "archive"] => "PATCH" "/api/v1/device/SER1/archive" as "/api/v1/device/{serial_number}/archive"),
    route!(["device", "SER1", "unarchive"] => "PATCH" "/api/v1/device/SER1/unarchive" as "/api/v1/device/{serial_number}/unarchive"),
    route!(["device", "SER1", "delete", "--confirm"] => "DELETE" "/api/v1/device/SER1?confirm=true" as "/api/v1/device/{serial_number}"),
    route!(["archive", "SER2"] => "PATCH" "/api/v1/device/SER2/archive" as "/api/v1/device/{serial_number}/archive"),
    route!(["delete", "SER2", "--confirm"] => "DELETE" "/api/v1/device/SER2?confirm=true" as "/api/v1/device/{serial_number}"),
    // Fleet module reports
    route!(["module", "hardware", "--include-archived", "--limit", "50", "--offset", "5"]
        => "GET" "/api/v1/hardware?includeArchived=true&limit=50&offset=5" as "/api/v1/hardware"),
    route!(["module", "installs"] => "GET" "/api/v1/installs" as "/api/v1/installs"),
    route!(["module", "installs/full", "--param", "foo=bar"] => "GET" "/api/v1/installs/full?foo=bar" as "/api/v1/installs/full"),
    route!(["module", "installs/filters"] => "GET" "/api/v1/installs/filters" as "/api/v1/installs/filters"),
    route!(["module", "network", "--limit", "1000"] => "GET" "/api/v1/network?limit=1000" as "/api/v1/network"),
    route!(["module", "security"] => "GET" "/api/v1/security" as "/api/v1/security"),
    route!(["module", "management"] => "GET" "/api/v1/management" as "/api/v1/management"),
    route!(["module", "inventory"] => "GET" "/api/v1/inventory" as "/api/v1/inventory"),
    route!(["module", "system"] => "GET" "/api/v1/system" as "/api/v1/system"),
    route!(["module", "peripherals"] => "GET" "/api/v1/peripherals" as "/api/v1/peripherals"),
    route!(["module", "identity"] => "GET" "/api/v1/identity" as "/api/v1/identity"),
    route!(["module", "profiles"] => "GET" "/api/v1/profiles" as "/api/v1/profiles"),
    route!(["dashboard", "--events-limit", "50", "--include-archived"]
        => "GET" "/api/v1/dashboard?eventsLimit=50&includeArchived=true" as "/api/v1/dashboard"),
    route!(["certificates", "--search", "acme", "--status", "expiring", "--limit", "20"]
        => "GET" "/api/v1/security/certificates?search=acme&status=expiring&limit=20" as "/api/v1/security/certificates"),
    route!(["logs", "munki", "--levels", "error,warning", "--platform", "macos", "--file", "run.log",
            "--grep", "timed out", "--summary", "--max-lines-per-device", "50", "--include-archived",
            "--limit", "10", "--offset", "0"]
        => "GET" "/api/v1/management/logs/munki?levels=error%2Cwarning&platform=macos&file=run.log&q=timed%20out&summary=true&maxLinesPerDevice=50&includeArchived=true&limit=10&offset=0"
        as "/api/v1/management/logs/{tool}"),
    // Applications
    route!(["apps", "list", "--names", "Zoom,Slack", "--publishers", "Zoom", "--search", "zo", "--size-min", "100",
            "--usages", "lab", "--platforms", "macos", "--load-all", "--limit", "10", "--device-limit", "100"]
        => "GET" "/api/v1/applications?applicationNames=Zoom%2CSlack&publishers=Zoom&search=zo&sizeMin=100&usages=lab&platforms=macos&loadAll=true&limit=10&deviceLimit=100"
        as "/api/v1/applications"),
    route!(["apps", "filters", "--include-archived"] => "GET" "/api/v1/applications/filters?includeArchived=true" as "/api/v1/applications/filters"),
    route!(["applications", "usage", "--days", "90", "--names", "Zoom", "--min-hours", "1.5", "--min-launches", "2",
            "--catalogs", "prod", "--rooms", "101"]
        => "GET" "/api/v1/applications/usage?days=90&applicationNames=Zoom&minHours=1.5&minLaunches=2&catalogs=prod&rooms=101"
        as "/api/v1/applications/usage"),
    route!(["apps", "by-device", "Zoom", "--days", "7", "--locations", "North Van"]
        => "GET" "/api/v1/applications/usage/by-device?app=Zoom&days=7&locations=North%20Van" as "/api/v1/applications/usage/by-device"),
    route!(["apps", "distribution", "Zoom,Slack", "--areas", "Design", "--fleets", "Lab"]
        => "GET" "/api/v1/applications/distribution?applicationNames=Zoom%2CSlack&areas=Design&fleets=Lab" as "/api/v1/applications/distribution"),
    route!(["apps", "collection-health", "--fresh-days", "3", "--stale-days", "60"]
        => "GET" "/api/v1/applications/collection-health?freshDays=3&staleDays=60" as "/api/v1/applications/collection-health"),
    // Events
    route!(["events", "--limit", "20"] => "GET" "/api/v1/events?limit=20" as "/api/v1/events"),
    route!(["events", "--offset", "5", "--since", "2026-09-01", "--until", "2026-09-09", "--type", "warning"]
        => "GET" "/api/v1/events?offset=5&startDate=2026-09-01&endDate=2026-09-09&type=warning" as "/api/v1/events"),
    route!(["events", "failures", "--limit", "5", "--serial", "ABC", "--reason", "upload_aborted", "--hours", "24", "--outcome", "all"]
        => "GET" "/api/v1/events/failures?limit=5&serial=ABC&reason=upload_aborted&hours=24&outcome=all" as "/api/v1/events/failures"),
    route!(["events", "payload", "12345"] => "GET" "/api/v1/events/12345/payload" as "/api/v1/events/{event_id}/payload"),
    route!(["events", "submit", r#"{"metadata":{"serialNumber":"X"}}"#] => "POST" "/api/v1/events" as "/api/v1/events"),
    // Health
    route!(["health"] => "GET" "/api/v1/health/live" as "/api/v1/health/live"),
    route!(["health", "--ready"] => "GET" "/api/v1/health/ready" as "/api/v1/health/ready"),
    route!(["health", "--full"] => "GET" "/api/v1/health" as "/api/v1/health"),
    route!(["metrics"] => "GET" "/api/v1/metrics" as "/api/v1/metrics"),
    route!(["negotiate", "--device", "cli"] => "GET" "/api/v1/negotiate?device=cli" as "/api/v1/negotiate"),
    // API keys
    route!(["api-keys", "list"] => "GET" "/api/v1/admin/api-keys" as "/api/v1/admin/api-keys"),
    route!(["api-keys", "create", "ci", "--scope", "read", "--scope", "admin"] => "POST" "/api/v1/admin/api-keys" as "/api/v1/admin/api-keys"),
    route!(["api-keys", "revoke", "k1"] => "DELETE" "/api/v1/admin/api-keys/k1" as "/api/v1/admin/api-keys/{key_id}"),
    // Admin
    route!(["admin", "usage-history", "date-anomalies", "--floor", "2025-01-01", "--limit", "5"]
        => "GET" "/api/v1/admin/usage-history/date-anomalies?floor=2025-01-01&limit=5" as "/api/v1/admin/usage-history/date-anomalies"),
    route!(["admin", "usage-history", "integrity", "--days", "14", "--sample", "10"]
        => "GET" "/api/v1/admin/usage-history/integrity?days=14&sample=10" as "/api/v1/admin/usage-history/integrity"),
    route!(["admin", "usage-history", "export", "--from", "2026-08-01", "--to", "2026-09-01"]
        => "GET" "/api/v1/admin/usage-history/export?from=2026-08-01&to=2026-09-01" as "/api/v1/admin/usage-history/export"),
    route!(["admin", "usage-history", "reset-baseline", "--before", "2026-01-01", "--reason", "term reset"]
        => "POST" "/api/v1/admin/usage-history/reset-baseline?before=2026-01-01&reason=term%20reset" as "/api/v1/admin/usage-history/reset-baseline"),
    route!(["admin", "usage-history", "reset-baseline", "--before", "2026-01-01", "--confirm"]
        => "POST" "/api/v1/admin/usage-history/reset-baseline?before=2026-01-01&confirm=true" as "/api/v1/admin/usage-history/reset-baseline"),
    route!(["admin", "usage-history", "cleanup", "--months", "12"]
        => "DELETE" "/api/v1/admin/usage-history/cleanup?months=12" as "/api/v1/admin/usage-history/cleanup"),
    route!(["admin", "cleanup-usage"] => "DELETE" "/api/v1/admin/usage-history/cleanup?months=18" as "/api/v1/admin/usage-history/cleanup"),
    route!(["admin", "installs", "clear-errors", "--days", "0", "--item-age-days", "3.5"]
        => "DELETE" "/api/v1/admin/installs/clear-errors?days=0&item_age_days=3.5" as "/api/v1/admin/installs/clear-errors"),
    route!(["admin", "clear-errors", "--days", "5"] => "DELETE" "/api/v1/admin/installs/clear-errors?days=5" as "/api/v1/admin/installs/clear-errors"),
    route!(["admin", "installs", "reclassify", "--batch", "500", "--limit", "1000"]
        => "POST" "/api/v1/admin/installs/reclassify?batch=500&limit=1000" as "/api/v1/admin/installs/reclassify"),
    route!(["admin", "debug-database"] => "GET" "/api/v1/debug/database" as "/api/v1/debug/database"),
    // Settings
    route!(["settings", "get"] => "GET" "/api/v1/settings" as "/api/v1/settings"),
    route!(["settings", "set", r#"{"a":1}"#, "--updated-by", "rod"] => "PUT" "/api/v1/settings" as "/api/v1/settings"),
    route!(["settings", "discover", "--include-archived"]
        => "GET" "/api/v1/settings/inventory/discover?include_archived=true" as "/api/v1/settings/inventory/discover"),
    // Raw
    route!(["raw", "/"] => "GET" "/" as "/"),
    route!(["raw", "/api/v1/dashboard", "--param", "eventsLimit=1"] => "GET" "/api/v1/dashboard?eventsLimit=1" as "/api/v1/dashboard"),
    route!(["raw", "/api/v1/admin/installs/reclassify", "-X", "post"] => "POST" "/api/v1/admin/installs/reclassify" as "/api/v1/admin/installs/reclassify"),
    route!(["raw", "/api/v1/settings", "-X", "put", "-d", r#"{"b":2}"#] => "PUT" "/api/v1/settings" as "/api/v1/settings"),
];

/// Operations the API deliberately keeps out of its schema
/// (`include_in_schema=False`) that the CLI still reaches.
const NOT_IN_SPEC: &[(&str, &str)] = &[("GET", "/api/v1/metrics")];

#[test]
fn every_command_sends_the_documented_request() {
    let api = MockApi::start();
    let mut failures = Vec::new();
    for route in ROUTES {
        let output = reportmate(&api, route.args);
        if !output.status.success() {
            failures.push(format!(
                "{:?}: exit {} — {}",
                route.args,
                output.status,
                stderr(&output).trim()
            ));
            continue;
        }
        let sent = api.last();
        if sent.method != route.method || sent.target != route.target {
            failures.push(format!(
                "{:?}: sent {} {} but expected {} {}",
                route.args, sent.method, sent.target, route.method, route.target
            ));
        }
        if sent.header("x-api-key") != Some("rm_test_key") {
            failures.push(format!("{:?}: X-API-Key not sent", route.args));
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

#[test]
fn every_api_operation_has_a_command() {
    let spec: serde_json::Value =
        serde_json::from_str(include_str!("openapi.json")).expect("tests/openapi.json");
    let mut spec_ops: BTreeSet<(String, String)> = BTreeSet::new();
    for (path, item) in spec["paths"].as_object().expect("paths") {
        for method in item.as_object().expect("path item").keys() {
            if ["get", "post", "put", "patch", "delete"].contains(&method.as_str()) {
                spec_ops.insert((method.to_ascii_uppercase(), path.clone()));
            }
        }
    }
    let covered: BTreeSet<(String, String)> = ROUTES
        .iter()
        .map(|r| (r.method.to_string(), r.template.to_string()))
        .collect();
    let not_in_spec: BTreeSet<(String, String)> = NOT_IN_SPEC
        .iter()
        .map(|(m, p)| (m.to_string(), p.to_string()))
        .collect();

    let missing: Vec<_> = spec_ops.difference(&covered).collect();
    let stale: Vec<_> = covered
        .difference(&spec_ops)
        .filter(|op| !not_in_spec.contains(op))
        .collect();

    assert!(
        missing.is_empty(),
        "API operations with no CLI command (add a command and a ROUTES row):\n{}",
        missing
            .iter()
            .map(|(m, p)| format!("  {m:6} {p}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert!(
        stale.is_empty(),
        "CLI commands for operations the API no longer has:\n{}",
        stale
            .iter()
            .map(|(m, p)| format!("  {m:6} {p}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert!(
        spec_ops.len() >= 50,
        "spec looks truncated: {} operations",
        spec_ops.len()
    );
}

#[test]
fn post_and_put_bodies_are_the_documented_json() {
    let api = MockApi::start();

    assert!(reportmate(
        &api,
        &["api-keys", "create", "ci", "--scope", "read", "--scope", "admin"]
    )
    .status
    .success());
    let sent = api.last();
    let body: serde_json::Value = serde_json::from_str(&sent.body).expect("json body");
    assert_eq!(
        body,
        serde_json::json!({"client_id": "ci", "scopes": ["read", "admin"]})
    );
    assert!(sent
        .header("content-type")
        .unwrap_or("")
        .starts_with("application/json"));

    assert!(reportmate(
        &api,
        &["settings", "set", r#"{"a":1}"#, "--updated-by", "rod"]
    )
    .status
    .success());
    let sent = api.last();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&sent.body).unwrap(),
        serde_json::json!({"a": 1})
    );
    assert_eq!(sent.header("x-updated-by"), Some("rod"));

    let dir = std::env::temp_dir().join(format!("reportmate-routes-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("payload.json");
    std::fs::write(&file, r#"{"metadata":{"serialNumber":"X"},"events":[]}"#).unwrap();
    let arg = format!("@{}", file.display());
    assert!(reportmate(&api, &["events", "submit", &arg])
        .status
        .success());
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&api.last().body).unwrap(),
        serde_json::json!({"metadata": {"serialNumber": "X"}, "events": []})
    );

    let output = reportmate(&api, &["settings", "set", "not json"]);
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("valid JSON"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn credentials_map_to_the_api_headers() {
    let api = MockApi::start();

    assert!(
        reportmate_env(&api, &["health"], &[("REPORTMATE_API_KEY", "rm_k")])
            .status
            .success()
    );
    let sent = api.last();
    assert_eq!(sent.header("x-api-key"), Some("rm_k"));
    assert_eq!(sent.header("authorization"), None);
    assert_eq!(sent.header("x-client-passphrase"), None);

    assert!(
        reportmate_env(&api, &["health"], &[("REPORTMATE_TOKEN", "jwt")])
            .status
            .success()
    );
    assert_eq!(api.last().header("authorization"), Some("Bearer jwt"));

    assert!(
        reportmate_env(&api, &["health"], &[("REPORTMATE_PASSPHRASE", "pp")])
            .status
            .success()
    );
    assert_eq!(api.last().header("x-client-passphrase"), Some("pp"));

    // Preference order: key beats token beats passphrase.
    assert!(reportmate_env(
        &api,
        &["health"],
        &[
            ("REPORTMATE_API_KEY", "rm_k"),
            ("REPORTMATE_TOKEN", "jwt"),
            ("REPORTMATE_PASSPHRASE", "pp")
        ]
    )
    .status
    .success());
    let sent = api.last();
    assert_eq!(sent.header("x-api-key"), Some("rm_k"));
    assert_eq!(sent.header("authorization"), None);

    assert!(reportmate_env(
        &api,
        &["settings", "get"],
        &[
            ("REPORTMATE_API_KEY", "rm_k"),
            ("REPORTMATE_INTERNAL_SECRET", "sek")
        ]
    )
    .status
    .success());
    assert_eq!(api.last().header("x-internal-secret"), Some("sek"));

    let output = reportmate_env(&api, &["health"], &[]);
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("no credential"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn api_errors_fail_with_the_upstream_status() {
    let api = MockApi::start();
    let output = reportmate(&api, &["raw", "/api/v1/forbidden"]);
    assert!(!output.status.success());
    let err = stderr(&output);
    assert!(err.contains("GET /api/v1/forbidden -> 403"), "{err}");
    assert!(err.contains("missing scope"), "{err}");
}

#[test]
fn text_endpoints_print_their_body_verbatim() {
    let api = MockApi::start();
    let output = reportmate(&api, &["metrics"]);
    assert!(output.status.success());
    assert_eq!(stdout(&output), "rm_up 1\n");

    let output = reportmate(
        &api,
        &[
            "admin",
            "usage-history",
            "export",
            "--from",
            "2026-08-01",
            "--to",
            "2026-09-01",
        ],
    );
    assert!(output.status.success());
    assert_eq!(stdout(&output), "a,b\n1,2\n3,4\n");

    let dir = std::env::temp_dir().join(format!("reportmate-export-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let out = dir.join("august.csv");
    let output = reportmate(
        &api,
        &[
            "admin",
            "usage-history",
            "export",
            "--from",
            "2026-08-01",
            "--to",
            "2026-09-01",
            "--out",
            out.to_str().unwrap(),
        ],
    );
    assert!(output.status.success());
    assert_eq!(std::fs::read_to_string(&out).unwrap(), "a,b\n1,2\n3,4\n");
    assert!(
        stderr(&output).contains("wrote 2 rows"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn json_output_is_the_api_body_pretty_printed() {
    let api = MockApi::start();
    let output = reportmate(&api, &["device", "SER1", "--output", "json"]);
    assert!(output.status.success());
    let value: serde_json::Value = serde_json::from_str(&stdout(&output)).expect("stdout is JSON");
    assert_eq!(value["path"], "/api/v1/device/SER1");
}

#[test]
fn destructive_commands_refuse_without_confirm() {
    let api = MockApi::start();
    let output = reportmate(&api, &["device", "SER1", "delete"]);
    assert!(!output.status.success());
    assert!(stderr(&output).contains("--confirm"));
    assert!(
        api.log.lock().unwrap().is_empty(),
        "no request may be sent without --confirm"
    );
}
