---
name: ynab-cli
description: Use when working with the ynab-agent-cli project or when a user wants YNAB data and actions through the local `ynab` CLI instead of direct MCP tools. Prefer this skill for agent-assisted terminal workflows, scripting, auth setup, transaction search, plan/account/category/payee management, and generating MCP config from the CLI.
---

# YNAB CLI

Use this skill when the repository or environment includes the `ynab` executable from `ynab-agent-cli`.

Default to the CLI path. Only switch to the MCP server path when the user explicitly asks for MCP, wants to wire the project into an MCP-capable client, or needs persistent tool-style integration instead of terminal commands.

## Workflow

1. Confirm the CLI is available with `ynab --help` or run through Cargo with `cargo run -p ynab-cli -- ...` when working from the repo checkout.
2. Check auth before plan-aware work with `ynab auth status`.
3. If a plan is not explicit, inspect available plans with `ynab plans list`.
4. Prefer read commands first to discover ids and current state before proposing or executing writes.
5. For writes, remember the CLI prompts interactively. Use `--yes` only when the user has clearly asked to make the change or when running a reviewed non-interactive command.

## Command patterns

- Read auth and profile state:
  `ynab auth status`
  `ynab auth whoami`
- Read plan data:
  `ynab plans list`
  `ynab plans settings PLAN_ID`
  `ynab plans set-default PLAN_ID`
- Read common resources:
  `ynab accounts list`
  `ynab categories list`
  `ynab payees list`
  `ynab months get 2026-04`
- Search and filter transactions:
  `ynab transactions list --month 2026-04`
  `ynab transactions search --query amazon --month 2026-04`
  `ynab transactions list-account ACCOUNT_ID --since-date 2026-04-01`
- Create or update data:
  `ynab --yes payees create --name "New Payee"`
  `ynab --yes categories update CATEGORY_ID --name "Renamed Category"`
  `ynab --yes transactions create --account-name "Checking" --date 2026-05-01 --amount 12.34 --payee-name "Coffee Shop"`
- Bulk and automation-friendly flows:
  `ynab transactions export --month 2026-04 --output imported.json`
  `ynab --yes transactions import --input imported.json`
  `ynab --output jsonl --transform transactions transactions list --month 2026-04`

## Output guidance

The CLI is JSON-first. Prefer:

- default JSON for tool consumption
- `--output pretty-json` when the user wants readable terminal output
- `--transform PATH` to narrow responses
- `--raw-output` when the transformed value is a scalar that should print without JSON quotes

When the user only needs one field, use transforms instead of post-processing large payloads in the model.

## Auth and secrets

- Stored auth can come from token login, OAuth login, or a one-shot `YNAB_ACCESS_TOKEN` / `--access-token` override.
- Avoid echoing secrets back to the user.
- `ynab auth login` is the preferred interactive OAuth path when the app is already configured.
- Use `--no-keyring` only when the environment cannot use the OS keyring and a file-backed fallback is acceptable.

## MCP handoff

When the user wants the MCP-server path, keep using the CLI as the setup assistant:

- Diagnose readiness with `ynab mcp doctor --project /absolute/project/path`
- Generate MCP client config snippets with `ynab mcp print-config --project /absolute/project/path`

Prefer the generated config snippets over hand-written MCP configuration.

## Repo-local note

In this repository, the CLI crate lives at `crates/ynab-cli` and the MCP server crate lives at `crates/ynab-mcp`. If the `ynab` binary is not installed yet, run commands through Cargo from the repo root.
