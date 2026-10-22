# ReportMate CLI wiki

`reportmateutil` is ReportMate's admin command-line client. It reads and manages the fleet over the API's `/api/v1` HTTP contract from an administrator's workstation, and gives scripts and coding agents a stable JSON interface. It is separate from the device agents, which run on managed endpoints and submit check-ins.

## Read first

- [Install, configuration and every command](../README.md)
- [Command definitions](../src/cli.rs)
- [Credential and endpoint resolution](../src/config.rs)
- [Route contract tests](../tests/routes.rs)

## Architecture

The CLI is one Rust binary built with clap, reqwest over rustls and tokio. It has no platform-specific behaviour, so the same source builds for macOS, Windows and Linux.

[`src/cli.rs`](../src/cli.rs) declares every command and flag, and stamps the release version from the `RM_VERSION` build environment, falling back to the Cargo package version. [`src/main.rs`](../src/main.rs) maps each command to an API method, path and query. [`src/client.rs`](../src/client.rs) sends the request and keeps a non-JSON body, such as a CSV export or Prometheus metrics, as text. [`src/output.rs`](../src/output.rs) prints tables for people and pretty JSON for `--output json`.

[`src/config.rs`](../src/config.rs) resolves the endpoint and credential from the first source that has them: the environment, the ReportMate app's saved connection, the device runner's preferences, then the deployment's cloud sign-in. `REPORTMATE_NO_DISCOVERY=1` limits resolution to the environment. Responses deserialize as untyped JSON, and `raw` reaches any path the typed commands do not yet cover.

## Development and verification

Build with `cargo build --release`; the binary is `target/release/reportmateutil`. The [ci workflow](../.github/workflows/ci.yml) runs the same checks a pull request must pass:

```
cargo fmt --all -- --check
```

```
cargo clippy --all-targets -- -D warnings
```

```
cargo test --locked
```

[`tests/routes.rs`](../tests/routes.rs) starts a recording mock of the API, runs every command against it, and asserts the exact method, path, query and body each one sends. A parity test checks the same table against [`tests/openapi.json`](../tests/openapi.json), so an API operation without a command fails the suite. The [openapi-sync workflow](../.github/workflows/openapi-sync.yml) regenerates that spec from the API source weekly and opens a pull request when it changes.

The [build workflow](../.github/workflows/build.yml) builds unsigned binaries for every target on each push and pull request, including a universal macOS binary and an unsigned installer package. A `v` tag publishes them as one release after every platform has built.

## Documentation ownership

Keep this wiki as the entry point and update the linked pages where the facts belong: commands and configuration in the [README](../README.md), behaviour in the source comments. Examples use fictional data. Deployment inventories, credentials, provider mappings and organization-specific operations belong with their deployment owner.
