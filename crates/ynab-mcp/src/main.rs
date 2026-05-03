mod server;
mod tools;
mod types;

use std::sync::Arc;

use anyhow::Result;
use clap::Parser;
use rmcp::{ServiceExt, transport::stdio};
use tokio::sync::Mutex;
use ynab_core::{AppState, OutputFormat, RuntimeOptions};

use crate::server::YnabMcpServer;

#[derive(Debug, Parser)]
#[command(name = "ynab-mcp")]
#[command(about = "MCP server for local YNAB access")]
struct Cli {
    #[arg(long)]
    profile: Option<String>,
    #[arg(long)]
    base_url: Option<String>,
    #[arg(long, action = clap::ArgAction::SetTrue)]
    no_keyring: bool,
    #[arg(long, env = "YNAB_ACCESS_TOKEN", hide_env_values = true)]
    access_token: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let app = AppState::load(RuntimeOptions {
        profile: cli.profile,
        use_keyring: !cli.no_keyring,
        base_url_override: cli.base_url,
        output_format: OutputFormat::Json,
        access_token_override: cli.access_token,
        access_token_override_source: access_token_source_from_args(),
    })?;

    let service = YnabMcpServer::new(Arc::new(Mutex::new(app)))
        .serve(stdio())
        .await?;
    service.waiting().await?;
    Ok(())
}

fn access_token_source_from_args() -> Option<&'static str> {
    for arg in std::env::args_os() {
        if arg == "--access-token" {
            return Some("flag");
        }
        if arg
            .to_str()
            .map(|value| value.starts_with("--access-token="))
            .unwrap_or(false)
        {
            return Some("flag");
        }
    }

    if std::env::var("YNAB_ACCESS_TOKEN").is_ok() {
        Some("env")
    } else {
        None
    }
}
