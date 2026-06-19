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

The CLI reads its target and credentials from the environment:

```
export REPORTMATE_API_URL=https://api.reportmate.app
export REPORTMATE_PASSPHRASE=your-passphrase
```

## Usage

List the fleet:

```
reportmate devices
```

Show one device:

```
reportmate device 0F33V9G25083HJ
```

Machine-readable output for scripting:

```
reportmate devices --output json
```

## Design

The CLI is a thin, typed client over the ReportMate REST API (`/api/v1/*`). It is intentionally platform-agnostic — there is no platform-specific behaviour, so one binary serves macOS, Windows, and Linux admins. Responses currently deserialize as untyped JSON; a future revision will generate a typed client from the API's published OpenAPI spec.

## License

AGPL-3.0-or-later. A commercial license is available — see [COMMERCIAL-LICENSE.md](COMMERCIAL-LICENSE.md).
