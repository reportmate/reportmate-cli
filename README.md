# reportmate-cli

The admin command-line interface for [ReportMate](https://reportmate.app). Query and manage your device fleet from the terminal — a single cross-platform binary that talks to the ReportMate API over HTTPS.

This is the admin tool (runs on your workstation). It is distinct from the device agents (`reportmate-client-mac`, `reportmate-client-win`), which run on managed endpoints.

## Install

Download the binary for your platform from [Releases](https://github.com/reportmate/reportmate-cli/releases), or build from source:

```
cargo build --release
```

The binary is `target/release/reportmate`.

## Configure

The CLI reads its target and credentials from the environment. A scoped API key is the preferred credential; the shared client passphrase also works:

```
export REPORTMATE_API_URL=https://api.reportmate.app
export REPORTMATE_API_KEY=rm_yourclient_yoursecret
```

```
export REPORTMATE_PASSPHRASE=your-passphrase
```

## Usage

List the fleet (add `--limit`, `--offset`, `--include-archived` to page):

```
reportmate devices
```

Show one device, or just one of its module documents:

```
reportmate device 0F33V9G25083HJ
```

```
reportmate device 0F33V9G25083HJ --module installs
```

Fleet-wide report for any module — `hardware`, `applications`, `installs`, `network`, `security`, `management`, `inventory`, `system`, `peripherals`, `identity` — including nested variants, with arbitrary query parameters passed through:

```
reportmate module hardware
```

```
reportmate module installs/full --param limit=100
```

Recent fleet events:

```
reportmate events --limit 20
```

API health, including the database readiness probe:

```
reportmate health --ready
```

Manage per-client API keys (requires the admin scope):

```
reportmate api-keys list
```

```
reportmate api-keys create ci-reader --scope read
```

```
reportmate api-keys revoke <key-id>
```

Any other GET endpoint, straight through:

```
reportmate raw /api/v1/dashboard
```

## Scripting and agents

Every command takes `--output json` and prints pretty JSON on stdout, which makes the CLI the simplest reliable way for scripts and coding agents to read the fleet:

```
reportmate devices --output json | jq '.devices[].serialNumber'
```

Commands that have no table view (device detail, module reports, raw) always print JSON. Errors go to stderr with the upstream status and body, and the exit code is non-zero on any failure.

## Design

The CLI is a thin client over the ReportMate REST API (`/api/v1/*`). It is intentionally platform-agnostic — there is no platform-specific behaviour, so one binary serves macOS, Windows, and Linux admins. Responses currently deserialize as untyped JSON; a future revision will generate a typed client from the API's published OpenAPI spec.

## License

AGPL-3.0-or-later. A commercial license is available — see [COMMERCIAL-LICENSE.md](COMMERCIAL-LICENSE.md).
