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

Contributors can also run commands from the checkout without installing:

```bash
cargo run -p ynab-cli -- plans list
```

Set a personal access token:

```bash
ynab auth token set --token "$YNAB_ACCESS_TOKEN"
```

For CI, scripts, or agent runs, use a token for only one invocation without saving it:

```bash
YNAB_ACCESS_TOKEN="your-token" ynab plans list
ynab --access-token "$YNAB_ACCESS_TOKEN" plans list
```

When authentication is established and no default plan is set yet, the CLI will automatically choose the most recently updated plan and persist it as the default for the active profile. If that did not happen previously, plan-aware commands still fall back to the same auto-selection behavior.

List plans:

```bash
ynab plans list
ynab plans list --include-accounts
ynab plans settings PLAN_ID
```

Get or create accounts:

```bash
ynab accounts get ACCOUNT_ID
ynab --yes accounts create --name "Checking" --account-type checking --balance 1000.00
```

List transactions for a specific month or apply local filters:

```bash
ynab transactions list --month 2026-04
ynab transactions list --startdate 2026-04-11 --enddate 2026-04-18 --uncleared-only
ynab transactions get TRANSACTION_ID
ynab --yes transactions delete TRANSACTION_ID
ynab transactions list-account ACCOUNT_ID --since-date 2026-04-01 --transaction-type unapproved
ynab transactions search --query amazon --month 2026-04
ynab transactions search --payee "Costco" --memo fuel --startdate 2026-04-01
ynab --yes transactions create-bulk --input transactions.json
ynab --yes transactions update-bulk --input updates.json
ynab --yes transactions import --input imported.json
ynab transactions export --month 2026-04 --output imported.json
```

Bulk transaction commands default `--input` to stdin, so these are equivalent:

```bash
ynab --yes transactions import --input imported.json
ynab --yes transactions import < imported.json
```

Create or update a payee:

```bash
ynab --yes payees create --name "New Payee"
ynab --yes payees update PAYEE_ID --name "Renamed Payee"
```

Create or update a category:

```bash
ynab --yes categories create --name "New Category" --group-id CATEGORY_GROUP_ID
ynab --yes categories update CATEGORY_ID --name "Renamed Category"
ynab categories get CATEGORY_ID
ynab --yes categories update-month 2026-04 CATEGORY_ID --budgeted 150.00
ynab --yes category-groups create --name "New Group"
ynab --yes category-groups update CATEGORY_GROUP_ID --name "Renamed Group"
```

Work with months, scheduled transactions, and location data:

```bash
ynab months list
ynab months get 2026-04
ynab scheduled-transactions list
ynab --yes scheduled-transactions create --account-name "Checking" --date 2026-05-01 --amount 125.00 --frequency monthly --payee-name "Rent"
ynab money-movements list
ynab money-movement-groups list-month 2026-04
ynab payee-locations list
ynab payee-locations list-payee PAYEE_ID
ynab user get
```

Start OAuth setup for a distributable integration:

```bash
ynab auth oauth configure \
  --client-id YOUR_CLIENT_ID \
  --client-secret YOUR_CLIENT_SECRET \
  --redirect-uri http://127.0.0.1:8765/callback

ynab auth login
```

`auth login` starts a temporary loopback callback server, waits for you to press Enter, opens the browser, captures the YNAB OAuth redirect, exchanges the code, and stores the resulting access and refresh tokens.

Check active authentication and storage:

```bash
ynab auth status
```

If you want the older manual flow, these commands still work:

```bash
ynab auth oauth start --open-browser
ynab auth oauth exchange --code YOUR_AUTH_CODE
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
ynab --transform plans plans list
ynab --output jsonl --transform transactions transactions list --month 2026-04
ynab --transform default_plan_id --raw-output auth status
```

Write commands prompt before changing YNAB data. In non-interactive environments, pass `--yes` after reviewing the command.

## OAuth Redirect URI

For `ynab auth login`, configure your YNAB OAuth application with a loopback redirect URI such as:

```text
http://127.0.0.1:8765/callback
```

The interactive login flow currently requires an `http://` redirect URI bound to `127.0.0.1`, `localhost`, or `::1`, with an explicit port.
