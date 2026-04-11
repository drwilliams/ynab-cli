# ynab-agent-cli

`ynab-agent-cli` is a JSON-first command-line client for the YNAB API that is designed for local automation and AI-agent use.

## Status

This repository currently provides:

- personal access token auth
- OAuth app configuration and PKCE-backed authorization code flow
- secure secret storage via OS keyring, with a file-backed fallback when `--no-keyring` is used
- resource commands for plans, accounts, categories, payees, and transactions
- stable JSON success/error envelopes on stdout/stderr

The implementation targets the current YNAB `/plans/...` API surface. The `plans` command includes a `budgets` alias for familiarity with the legacy naming.

## Tooling

- Rust stable
- Cargo
- `clap` for the CLI
- `reqwest` + `tokio` for HTTP
- `serde` for JSON

## Quick start

Install Rust if needed, then:

```bash
rustup default stable
cargo build
```

Set a personal access token:

```bash
cargo run -p ynab-cli -- auth token set --token "$YNAB_ACCESS_TOKEN"
```

List plans:

```bash
cargo run -p ynab-cli -- plans list
```

Start OAuth setup for a distributable integration:

```bash
cargo run -p ynab-cli -- auth oauth configure \
  --client-id YOUR_CLIENT_ID \
  --client-secret YOUR_CLIENT_SECRET \
  --redirect-uri http://127.0.0.1:8765/callback

cargo run -p ynab-cli -- auth login
```

`auth login` starts a temporary loopback callback server, waits for you to press Enter, opens the browser, captures the YNAB OAuth redirect, exchanges the code, and stores the resulting access and refresh tokens.

If you want the older manual flow, these commands still work:

```bash
cargo run -p ynab-cli -- auth oauth start --open-browser
cargo run -p ynab-cli -- auth oauth exchange --code YOUR_AUTH_CODE
```

## Configuration

By default the CLI stores config and file-backed secrets under:

```text
~/.ynab-agent-cli/
```

To override explicitly in any environment, set:

```bash
export YNAB_AGENT_CLI_HOME=/absolute/path/to/runtime-home
```

That places config and file-backed secrets under that directory.

## OAuth Redirect URI

For `ynab auth login`, configure your YNAB OAuth application with a loopback redirect URI such as:

```text
http://127.0.0.1:8765/callback
```

The interactive login flow currently requires an `http://` redirect URI bound to `127.0.0.1`, `localhost`, or `::1`, with an explicit port.
