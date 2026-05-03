# ynab-agent-cli

`ynab-agent-cli` is an AI-agent-first command-line client for the YNAB API. It is built to work well for humans in a terminal, for local automation, and for agent tooling that wants stable JSON I/O instead of screen-scraping a traditional CLI.

This repository also includes a local MCP server, `ynab-mcp`, so MCP-capable clients can use the same YNAB profiles, auth state, and shared core logic without going through the CLI surface.

This repository now also ships a reusable agent skill at `skills/ynab-cli` so you can offer two integration paths:

- CLI + skill (preferred): Codex operates through the local `ynab` executable and shell commands.
- MCP server: Codex or another MCP-capable client talks to `ynab-mcp` over stdio.

## Status

This repository currently provides:

- an AI-agent-first CLI with stable JSON success and error envelopes
- a companion local MCP server for MCP-capable clients
- personal access token auth
- OAuth app configuration and PKCE-backed authorization code flow
- one-shot token overrides via `--access-token` or `YNAB_ACCESS_TOKEN`
- secure secret storage via OS keyring, with a file-backed fallback when `--no-keyring` is used
- command coverage for the current YNAB API families: plans, accounts, categories, category groups, payees, transactions, months, scheduled transactions, money movements, payee locations, and user
- optional transforms and JSON Lines output for scripting and agent workflows

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

## Two ways to use this project

### 1. CLI + skill (preferred)

This repo includes a distributable, vendor-neutral skill folder at:

```text
skills/ynab-cli
```

The core `SKILL.md` is designed to stay generic. The optional `agents/openai.yaml` file is only for Codex/OpenAI UI metadata.

Install paths by client:

```text
Codex:       ~/.codex/skills/ynab-cli
Claude Code: ~/.claude/skills/ynab-cli
OpenClaw:    <workspace>/skills/ynab-cli
```

For Codex, copy or symlink the full folder so `agents/openai.yaml` is preserved. For Claude Code and OpenClaw, the important file is `SKILL.md`; the extra Codex metadata can remain present and be ignored.

The CLI can install the bundled skill for you:

```bash
ynab skill install codex
ynab skill install claude
ynab skill install claude --project "$PWD"
ynab skill install openclaw
ynab skill install openclaw --project "$PWD"
ynab skill status
ynab skill status --project "$PWD"
```

`skill status` reports the resolved install locations and whether the bundled skill is already present. Project-scoped installs are currently supported for Claude Code and OpenClaw. Codex installs currently target the shared `~/.codex/skills` directory.

Once installed, invoke it explicitly with prompts such as:

```text
Use $ynab-cli to inspect my YNAB auth status and list plans.
Use $ynab-cli to find uncategorized transactions from April 2026.
Use $ynab-cli to help me configure MCP for this project, but keep the CLI path as the default workflow.
```

Claude Code also supports direct slash-style invocation using the skill name, typically `/ynab-cli`, when the skill is installed in its skill directory.

This path keeps the CLI as the primary interface, which is often the simplest option for local development, scripting, and transparent agent behavior across agents.

### 2. MCP server

If you want direct MCP integration instead, build the server and point your MCP client at `ynab-mcp`:

```bash
cargo build -p ynab-mcp
cargo run -p ynab-cli -- mcp doctor --project "$PWD"
cargo run -p ynab-cli -- mcp print-config --project "$PWD"
```

`mcp doctor` checks whether the server binary and auth state are ready. `mcp print-config` emits project-scoped Codex config and a `.mcp.json` snippet so clients can use generated config instead of hand-written setup.

Build the local MCP server from this checkout:

```bash
cargo build -p ynab-mcp
```

Contributors can also run commands from the checkout without installing:

```bash
cargo run -p ynab-cli -- plans list
```

Run the MCP server over stdio:

```bash
cargo run -p ynab-mcp --
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

## MCP Server

This repository now includes a local MCP server crate at `crates/ynab-mcp`.

Current tool coverage reuses the existing `ynab-core` methods for both read and common write operations:

- `ynab_auth_status`
- `ynab_whoami`
- `ynab_get_plan`
- `ynab_get_plan_settings`
- `ynab_set_default_plan`
- `ynab_list_plans`
- `ynab_list_accounts`
- `ynab_get_account`
- `ynab_create_account`
- `ynab_list_categories`
- `ynab_get_category`
- `ynab_create_category`
- `ynab_update_category`
- `ynab_update_month_category`
- `ynab_create_category_group`
- `ynab_update_category_group`
- `ynab_list_payees`
- `ynab_create_payee`
- `ynab_update_payee`
- `ynab_list_transactions`
- `ynab_search_transactions`
- `ynab_get_transaction`
- `ynab_create_transaction`
- `ynab_update_transaction`
- `ynab_delete_transaction`
- `ynab_create_transactions_bulk`
- `ynab_update_transactions_bulk`
- `ynab_import_transactions`
- `ynab_list_transactions_by_account`
- `ynab_list_transactions_by_category`
- `ynab_list_transactions_by_payee`
- `ynab_list_months`
- `ynab_get_month`
- `ynab_list_scheduled_transactions`
- `ynab_get_scheduled_transaction`
- `ynab_create_scheduled_transaction`
- `ynab_update_scheduled_transaction`
- `ynab_delete_scheduled_transaction`
- `ynab_list_money_movements`
- `ynab_list_money_movements_by_month`
- `ynab_list_money_movement_groups`
- `ynab_list_money_movement_groups_by_month`
- `ynab_list_payee_locations`
- `ynab_get_payee_location`
- `ynab_list_payee_locations_by_payee`
- `ynab_get_user`

The server reuses the same profile, runtime home, keyring behavior, `--base-url`, and one-shot access token override patterns as the CLI.

The CLI also includes a couple of MCP setup helpers:

```bash
ynab mcp print-config --project /absolute/path/to/project
ynab mcp doctor --project /absolute/path/to/project
```

`print-config` emits both a project-scoped Codex config snippet and a `.mcp.json` snippet. `doctor` checks the companion `ynab-mcp` binary, reports the current auth/profile state, and optionally inspects whether a project already has a local `.mcp.json`.

You can run it directly:

```bash
cargo run -p ynab-mcp -- --profile default
```

Or point an MCP client at the built binary:

```json
{
  "mcpServers": {
    "ynab": {
      "command": "/absolute/path/to/target/debug/ynab-mcp",
      "args": ["--profile", "default"]
    }
  }
}
```
