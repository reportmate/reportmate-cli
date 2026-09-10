# reportmate-cli

The admin command-line interface for [ReportMate](https://reportmate.app). Query and manage your device fleet from the terminal — a single cross-platform binary that talks to the ReportMate API over HTTPS.

This is the admin tool (runs on your workstation). It is distinct from the device agents (`reportmate-client-mac`, `reportmate-client-win`), which run on managed endpoints.

The CLI covers the whole `/api/v1` surface: every read, report, maintenance and settings endpoint has a command, and `raw` reaches anything that lands before the CLI catches up.

## Install

Download the binary for your platform from [Releases](https://github.com/reportmate/reportmate-cli/releases), or build from source:

```
cargo build --release
```

The binary is `target/release/reportmate`.

## Configure

The CLI reads its target and credentials from the environment. A scoped API key is the preferred credential. An OIDC bearer token from your identity provider or the shared client passphrase also work:

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
reportmate devices
```

Fleet-wide report for any module — `hardware`, `applications`, `installs`, `network`, `security`, `management`, `inventory`, `system`, `peripherals`, `identity`, `profiles` — including nested variants. `--include-archived`, `--limit` and `--offset` are typed; anything else passes through with `--param`:

```
reportmate module hardware --limit 100
```

```
reportmate module installs/full --param foo=bar
```

Dashboard rollup:

```
reportmate dashboard --events-limit 50
```

Certificates across the fleet (`--status` is `all`, `valid`, `expired` or `expiring`):

```
reportmate certificates --search "MS-Organization" --status expiring
```

Sweep one tool's log tails across every device. Levels default to error and warning; `--summary` folds the same fault on many devices into one pattern:

```
reportmate logs munki --levels error --grep "timed out" --summary
```

## Devices

Show one device, or just one of its module documents:

```
reportmate device C00EXAMPLE001
```

```
reportmate device C00EXAMPLE001 --module installs
```

Per-device subcommands: `info` (fast summary), `module NAME`, `events`, `installs-log`, `log TOOL`, `usage`:

```
reportmate device 0F33V9G25083HJ events --limit 20 --type error
```

```
reportmate device 0F33V9G25083HJ log munki
```

```
reportmate device 0F33V9G25083HJ usage --days 90 --app Photoshop
```

Lifecycle (admin scope). Delete refuses to run without `--confirm`:

```
reportmate device 0F33V9G25083HJ archive
```

```
reportmate device 0F33V9G25083HJ delete --confirm
```

## Applications

Installed applications with filters (`--names`, `--publishers`, `--search`, inventory facets such as `--catalogs` and `--platforms`, all comma-separated):

```
reportmate apps list --names "Zoom,Slack" --platforms macos --limit 200
```

Usage over a window, per-device usage of one app, distribution by inventory facet, available filter values, and collection freshness:

```
reportmate apps usage --days 30 --min-hours 1
```

```
reportmate apps by-device Photoshop --days 90
```

```
reportmate apps distribution "Zoom,Slack" --areas Design
```

```
reportmate apps filters
```

```
reportmate apps collection-health --fresh-days 7 --stale-days 30
```

## Events

Recent fleet events, with paging and filters:

```
reportmate events --limit 20 --type error --since 2026-09-01
```

Check-ins the API turned away (`--outcome` is `rejected`, `retried`, `accepted` or `all`):

```
reportmate events failures --hours 24
```

Full payload for one event, and submitting a check-in payload the way a device agent does:

```
reportmate events payload 11184392
```

```
reportmate events submit @checkin.json
```

## Health and diagnostics

Liveness by default; `--ready` adds the database probe, `--full` returns the complete report:

```
reportmate health --ready
```

Prometheus metrics, and the real-time negotiate (connection URL and token):

```
reportmate metrics
```

```
reportmate negotiate
```

## Admin (admin scope)

Manage per-client API keys:

```
reportmate api-keys list
```

```
reportmate api-keys create ci-reader --scope read
```

```
reportmate api-keys revoke <key-id>
```

Usage-history maintenance. `reset-baseline` previews unless `--confirm` is given; `export` streams CSV to stdout or `--out`:

```
reportmate admin usage-history date-anomalies
```

```
reportmate admin usage-history integrity --days 7
```

```
reportmate admin usage-history export --from 2026-08-01 --to 2026-09-01 --out august.csv
```

```
reportmate admin usage-history reset-baseline --before 2026-01-01 --reason "term reset" --confirm
```

```
reportmate admin usage-history cleanup --months 18
```

Managed-software installs maintenance:

```
reportmate admin installs clear-errors --days 10 --item-age-days 30
```

```
reportmate admin installs reclassify --limit 500
```

Database diagnostics (duplicates, orphans, bloat):

```
reportmate admin debug-database
```

## Settings

Read the org settings, replace them from a JSON document, or discover the inventory keys present in the fleet. The write and discovery calls need `REPORTMATE_INTERNAL_SECRET`:

```
reportmate settings get
```

```
reportmate settings set @settings.json --updated-by rod
```

```
reportmate settings discover
```

## Raw

Any path, any method, optional JSON body:

```
reportmate raw /api/v1/dashboard --param eventsLimit=10
```

```
reportmate raw /api/v1/admin/installs/reclassify -X post
```

```
reportmate raw /api/v1/settings -X put -d @settings.json
```

## Scripting and agents

Every command takes `--output json` and prints pretty JSON on stdout, which makes the CLI the simplest reliable way for scripts and coding agents to read the fleet:

```
reportmate devices --output json | jq '.devices[].serialNumber'
```

Commands that have no table view (device detail, module reports, raw) always print JSON. Text endpoints (`metrics`, `usage-history export`) print their body verbatim. Errors go to stderr with the upstream status and body, and the exit code is non-zero on any failure.

## Design

The CLI is a thin client over the ReportMate REST API (`/api/v1/*`). It is intentionally platform-agnostic — there is no platform-specific behaviour, so one binary serves macOS, Windows, and Linux admins. Responses currently deserialize as untyped JSON; a future revision will generate a typed client from the API's published OpenAPI spec.

## License

AGPL-3.0-or-later. A commercial license is available — see [COMMERCIAL-LICENSE.md](COMMERCIAL-LICENSE.md).
