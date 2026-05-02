# ynab-agent-cli

`ynab-agent-cli` is a JSON-first command-line client for the YNAB API that is designed for local automation and AI-agent use.

## Status

This repository currently provides:

- personal access token auth
- OAuth app configuration and PKCE-backed authorization code flow
- one-shot token overrides via `--access-token` or `YNAB_ACCESS_TOKEN`
- secure secret storage via OS keyring, with a file-backed fallback when `--no-keyring` is used
- command coverage for the current YNAB API families: plans, accounts, categories, category groups, payees, transactions, months, scheduled transactions, money movements, payee locations, and user
- stable JSON success/error envelopes on stdout/stderr, with optional transforms and JSON Lines output

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

Install the CLI from this checkout:

```bash
cargo install --path crates/ynab-cli
```

That installs the `ynab` executable into Cargo's bin directory, usually `~/.cargo/bin`. Make sure that directory is on your `PATH`, then verify:

```bash
ynab --help
```

To reinstall after pulling updates:

```bash
cargo install --path crates/ynab-cli --force
```

Set a personal access token:

```bash
cargo run -p ynab-cli -- auth token set --token "$YNAB_ACCESS_TOKEN"
```

For CI, scripts, or agent runs, use a token for only one invocation without saving it:

```bash
YNAB_ACCESS_TOKEN="your-token" cargo run -p ynab-cli -- plans list
cargo run -p ynab-cli -- --access-token "$YNAB_ACCESS_TOKEN" plans list
```

When authentication is established and no default plan is set yet, the CLI will automatically choose the most recently updated plan and persist it as the default for the active profile. If that did not happen previously, plan-aware commands still fall back to the same auto-selection behavior.

List plans:

```bash
cargo run -p ynab-cli -- plans list
cargo run -p ynab-cli -- plans list --include-accounts
cargo run -p ynab-cli -- plans settings PLAN_ID
```

Get or create accounts:

```bash
cargo run -p ynab-cli -- accounts get ACCOUNT_ID
cargo run -p ynab-cli -- --yes accounts create --name "Checking" --account-type checking --balance 1000.00
```

List transactions for a specific month or apply local filters:

```bash
cargo run -p ynab-cli -- transactions list --month 2026-04
cargo run -p ynab-cli -- transactions list --startdate 2026-04-11 --enddate 2026-04-18 --uncleared-only
cargo run -p ynab-cli -- transactions get TRANSACTION_ID
cargo run -p ynab-cli -- --yes transactions delete TRANSACTION_ID
cargo run -p ynab-cli -- transactions list-account ACCOUNT_ID --since-date 2026-04-01 --transaction-type unapproved
cargo run -p ynab-cli -- transactions search --query amazon --month 2026-04
cargo run -p ynab-cli -- transactions search --payee "Costco" --memo fuel --startdate 2026-04-01
cargo run -p ynab-cli -- --yes transactions create-bulk --input transactions.json
cargo run -p ynab-cli -- --yes transactions update-bulk --input updates.json
cargo run -p ynab-cli -- --yes transactions import --input imported.json
cargo run -p ynab-cli -- transactions export --month 2026-04 --output imported.json
```

Bulk transaction commands default `--input` to stdin, so these are equivalent:

```bash
cargo run -p ynab-cli -- --yes transactions import --input imported.json
cargo run -p ynab-cli -- --yes transactions import < imported.json
```

Create or update a payee:

```bash
cargo run -p ynab-cli -- --yes payees create --name "New Payee"
cargo run -p ynab-cli -- --yes payees update PAYEE_ID --name "Renamed Payee"
```

Create or update a category:

```bash
cargo run -p ynab-cli -- --yes categories create --name "New Category" --group-id CATEGORY_GROUP_ID
cargo run -p ynab-cli -- --yes categories update CATEGORY_ID --name "Renamed Category"
cargo run -p ynab-cli -- categories get CATEGORY_ID
cargo run -p ynab-cli -- --yes categories update-month 2026-04 CATEGORY_ID --budgeted 150.00
cargo run -p ynab-cli -- --yes category-groups create --name "New Group"
cargo run -p ynab-cli -- --yes category-groups update CATEGORY_GROUP_ID --name "Renamed Group"
```

Work with months, scheduled transactions, and location data:

```bash
cargo run -p ynab-cli -- months list
cargo run -p ynab-cli -- months get 2026-04
cargo run -p ynab-cli -- scheduled-transactions list
cargo run -p ynab-cli -- --yes scheduled-transactions create --account-name "Checking" --date 2026-05-01 --amount 125.00 --frequency monthly --payee-name "Rent"
cargo run -p ynab-cli -- money-movements list
cargo run -p ynab-cli -- money-movement-groups list-month 2026-04
cargo run -p ynab-cli -- payee-locations list
cargo run -p ynab-cli -- payee-locations list-payee PAYEE_ID
cargo run -p ynab-cli -- user get
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

Check active authentication and storage:

```bash
cargo run -p ynab-cli -- auth status
```

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

## Output controls

The default output is a compact JSON envelope. Use `--output pretty-json` for readable JSON, `--output jsonl` for line-delimited array output, `--transform PATH` to select a dotted JSON path, and `--raw-output` to print scalar transform results without JSON quoting.

```bash
cargo run -p ynab-cli -- --transform plans plans list
cargo run -p ynab-cli -- --output jsonl --transform transactions transactions list --month 2026-04
cargo run -p ynab-cli -- --transform default_plan_id --raw-output auth status
```

Write commands prompt before changing YNAB data. In non-interactive environments, pass `--yes` after reviewing the command.

## OAuth Redirect URI

For `ynab auth login`, configure your YNAB OAuth application with a loopback redirect URI such as:

```text
http://127.0.0.1:8765/callback
```

The interactive login flow currently requires an `http://` redirect URI bound to `127.0.0.1`, `localhost`, or `::1`, with an explicit port.
