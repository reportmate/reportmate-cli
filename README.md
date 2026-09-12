# reportmate-cli

`reportmateutil`, the admin command-line interface for [ReportMate](https://reportmate.app). Query and manage your device fleet from the terminal — a single cross-platform binary that talks to the ReportMate API over HTTPS.

The tool is named `reportmateutil` so that `reportmate` stays the name of the ReportMate app that ships it; on managed Macs it lives inside ReportMate.app with a `/usr/local/bin/reportmateutil` symlink, and on managed PCs it sits beside the app in `C:\Program Files\ReportMate`. This is the admin tool (runs on your workstation). It is distinct from the device agents (`reportmate-client-mac`, `reportmate-client-win`), which run on managed endpoints.

The CLI covers the whole `/api/v1` surface: every read, report, maintenance and settings endpoint has a command, and `raw` reaches anything that lands before the CLI catches up.

## Install

Download the binary for your platform from [Releases](https://github.com/reportmate/reportmate-cli/releases): one tarball per target, each holding a single `reportmateutil` binary, plus `ReportMateUtil-<version>.pkg`, an unsigned installer of the universal macOS binary. Every push and pull request builds the same set as workflow artifacts, and a `v<calendar-version>` tag publishes them as a release in one step once every platform has built. The ReportMate apps for Mac and Windows bundle the binary from these releases, so a managed machine gets it with the app. Or build from source:

```
cargo build --release
```

The binary is `target/release/reportmateutil`.

## Configure

On an admin machine that runs the ReportMate app there is nothing to configure: the CLI resolves its endpoint and credential from, in order, the environment, the app's saved connection, the device runner's preferences, and the deployment's cloud sign-in. `reportmateutil config` shows what it found and where each value came from.

- The app leaves its non-secret connection (endpoint, auth method, Entra audience) in `~/Library/Application Support/ReportMate/connection.json` on macOS and `%ProgramData%\ReportMate\connection.json` on Windows. An API key or passphrase comes from the app's macOS Keychain item (macOS asks once to allow the CLI); an Entra sign-in needs no secret, the CLI mints a token with `az account get-access-token` for the app's audience.
- Entra sign-in comes before any shared secret on the machine: the audience from `REPORTMATE_OIDC_AUDIENCE`, the app's connection, or the API's own `/api/v1/auth/config`, exchanged for a token with `az account get-access-token`. Nothing is stored.
- With no app and no Entra, the runner's own endpoint and credential are used: macOS `com.github.reportmate` preferences (endpoint and shared passphrase), Windows `HKLM\SOFTWARE\ReportMate` with its Settings and Policies subkeys (endpoint, `ReadApiKey`, then passphrase). The runner's ingest-only `ApiKey` is never used for reads.
- With neither, the deployment's cloud: an Entra audience in `REPORTMATE_OIDC_AUDIENCE` is exchanged through `az`; an AWS-hosted API reads the client passphrase from Secrets Manager (`REPORTMATE_AWS_SECRET_ID`, default `reportmate/client-passphrase`) with your `aws` session.

Set `REPORTMATE_NO_DISCOVERY=1` to confine resolution to the environment (CI, scripts that must not pick up the machine's connection).

Explicit configuration still wins. A scoped API key is the preferred credential. An OIDC bearer token from your identity provider or the shared client passphrase also work:

```
export REPORTMATE_API_URL=https://api.reportmate.app
export REPORTMATE_API_KEY=rm_yourclient_yoursecret
```

```
export REPORTMATE_TOKEN=eyJhbGciOi...
```

```
export REPORTMATE_PASSPHRASE=your-passphrase
```

Internal-only calls (`settings set`, `settings discover`) additionally need the internal secret:

```
export REPORTMATE_INTERNAL_SECRET=...
```

## Fleet

List the fleet (add `--limit`, `--offset`, `--include-archived` to page):

```
reportmateutil devices
```

Fleet-wide report for any module — `hardware`, `applications`, `installs`, `network`, `security`, `management`, `inventory`, `system`, `peripherals`, `identity`, `profiles` — including nested variants. `--include-archived`, `--limit` and `--offset` are typed; anything else passes through with `--param`:

```
reportmateutil module hardware --limit 100
```

```
reportmateutil module installs/full --param foo=bar
```

Dashboard rollup:

```
reportmateutil dashboard --events-limit 50
```

Certificates across the fleet (`--status` is `all`, `valid`, `expired` or `expiring`):

```
reportmateutil certificates --search "MS-Organization" --status expiring
```

Sweep one tool's log tails across every device. Levels default to error and warning; `--summary` folds the same fault on many devices into one pattern:

```
reportmateutil logs munki --levels error --grep "timed out" --summary
```

## Devices

Show one device, or just one of its module documents:

```
reportmateutil device 0F33V9G25083HJ
```

```
reportmateutil device 0F33V9G25083HJ --module installs
```

Per-device subcommands: `info` (fast summary), `module NAME`, `events`, `installs-log`, `log TOOL`, `usage`:

```
reportmateutil device 0F33V9G25083HJ events --limit 20 --type error
```

```
reportmateutil device 0F33V9G25083HJ log munki
```

```
reportmateutil device 0F33V9G25083HJ usage --days 90 --app Photoshop
```

Lifecycle (admin scope). Delete refuses to run without `--confirm`:

```
reportmateutil device 0F33V9G25083HJ archive
```

```
reportmateutil device 0F33V9G25083HJ delete --confirm
```

## Applications

Installed applications with filters (`--names`, `--publishers`, `--search`, inventory facets such as `--catalogs` and `--platforms`, all comma-separated):

```
reportmateutil apps list --names "Zoom,Slack" --platforms macos --limit 200
```

Usage over a window, per-device usage of one app, distribution by inventory facet, available filter values, and collection freshness:

```
reportmateutil apps usage --days 30 --min-hours 1
```

```
reportmateutil apps by-device Photoshop --days 90
```

```
reportmateutil apps distribution "Zoom,Slack" --areas Design
```

```
reportmateutil apps filters
```

```
reportmateutil apps collection-health --fresh-days 7 --stale-days 30
```

## Events

Recent fleet events, with paging and filters:

```
reportmateutil events --limit 20 --type error --since 2026-09-01
```

Check-ins the API turned away (`--outcome` is `rejected`, `retried`, `accepted` or `all`):

```
reportmateutil events failures --hours 24
```

Full payload for one event, and submitting a check-in payload the way a device agent does:

```
reportmateutil events payload 11184392
```

```
reportmateutil events submit @checkin.json
```

## Health and diagnostics

Liveness by default; `--ready` adds the database probe, `--full` returns the complete report:

```
reportmateutil health --ready
```

Prometheus metrics, and the real-time negotiate (connection URL and token):

```
reportmateutil metrics
```

```
reportmateutil negotiate
```

## Admin (admin scope)

Manage per-client API keys:

```
reportmateutil api-keys list
```

```
reportmateutil api-keys create ci-reader --scope read
```

```
reportmateutil api-keys revoke <key-id>
```

Usage-history maintenance. `reset-baseline` previews unless `--confirm` is given; `export` streams CSV to stdout or `--out`:

```
reportmateutil admin usage-history date-anomalies
```

```
reportmateutil admin usage-history integrity --days 7
```

```
reportmateutil admin usage-history export --from 2026-08-01 --to 2026-09-01 --out august.csv
```

```
reportmateutil admin usage-history reset-baseline --before 2026-01-01 --reason "term reset" --confirm
```

```
reportmateutil admin usage-history cleanup --months 18
```

Managed-software installs maintenance:

```
reportmateutil admin installs clear-errors --days 10 --item-age-days 30
```

```
reportmateutil admin installs reclassify --limit 500
```

Database diagnostics (duplicates, orphans, bloat):

```
reportmateutil admin debug-database
```

## Settings

Read the org settings, replace them from a JSON document, or discover the inventory keys present in the fleet. The write and discovery calls need `REPORTMATE_INTERNAL_SECRET`:

```
reportmateutil settings get
```

```
reportmateutil settings set @settings.json --updated-by rod
```

```
reportmateutil settings discover
```

## Raw

Any path, any method, optional JSON body:

```
reportmateutil raw /api/v1/dashboard --param eventsLimit=10
```

```
reportmateutil raw /api/v1/admin/installs/reclassify -X post
```

```
reportmateutil raw /api/v1/settings -X put -d @settings.json
```

## Scripting and agents

Every command takes `--output json` and prints pretty JSON on stdout, which makes the CLI the simplest reliable way for scripts and coding agents to read the fleet:

```
reportmateutil devices --output json | jq '.devices[].serialNumber'
```

Commands that have no table view (device detail, module reports, raw) always print JSON. Text endpoints (`metrics`, `usage-history export`) print their body verbatim. Errors go to stderr with the upstream status and body, and the exit code is non-zero on any failure.

## Design

The CLI is a thin client over the ReportMate REST API (`/api/v1/*`). It is intentionally platform-agnostic — there is no platform-specific behaviour, so one binary serves macOS, Windows, and Linux admins. Responses currently deserialize as untyped JSON; a future revision will generate a typed client from the API's published OpenAPI spec.

## Contract tests

`cargo test` runs two guards beyond the unit tests. `tests/routes.rs` starts a recording mock of the API and runs every command against it, asserting the exact method, path and query each one sends. The same table names the OpenAPI operation each command implements, and a parity test checks it against `tests/openapi.json`, which is generated from the API repository's own source by the `openapi-sync` workflow (weekly, or on demand). A route added to the API fails the suite until the CLI has a command for it.

## License

AGPL-3.0-or-later. A commercial license is available — see [COMMERCIAL-LICENSE.md](COMMERCIAL-LICENSE.md).
