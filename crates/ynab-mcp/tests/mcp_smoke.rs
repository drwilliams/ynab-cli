use std::{
    collections::HashSet,
    io::{Read, Write},
    net::TcpListener,
    thread,
};

use rmcp::{
    ServiceExt,
    model::CallToolRequestParams,
    transport::TokioChildProcess,
};
use serde_json::{Value, json};
use tempfile::TempDir;

#[tokio::test]
async fn ynab_mcp_lists_tools_and_handles_basic_calls() -> anyhow::Result<()> {
    let temp_dir = TempDir::new()?;
    let base_url = spawn_json_server(
        json!({
            "data": {
                "plans": [
                    {
                        "id": "plan-123",
                        "name": "House Budget",
                        "last_modified_on": "2026-04-01T00:00:00Z"
                    }
                ],
                "server_knowledge": 77
            }
        })
        .to_string(),
    );

    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_ynab-mcp"));
    command.env("YNAB_AGENT_CLI_HOME", temp_dir.path());
    command.args([
        "--no-keyring",
        "--base-url",
        &base_url,
        "--access-token",
        "test-token",
    ]);

    let transport = TokioChildProcess::new(command)?;
    let client = ().serve(transport).await?;

    let tools = client.list_all_tools().await?;
    let tool_names = tools
        .iter()
        .map(|tool| tool.name.as_ref())
        .collect::<HashSet<_>>();

    assert!(
        tool_names.contains("ynab_auth_status"),
        "tools exposed by server: {tool_names:?}"
    );
    assert!(tool_names.contains("ynab_list_plans"));
    assert!(tool_names.contains("ynab_list_transactions"));
    assert!(tool_names.contains("ynab_get_user"));

    let auth_status = client
        .call_tool(CallToolRequestParams::new("ynab_auth_status"))
        .await?;
    assert_ne!(auth_status.is_error, Some(true));
    let auth_status_text = first_text(&auth_status)?;
    let auth_status_json: Value = serde_json::from_str(auth_status_text)?;
    assert_eq!(auth_status_json["auth_source"], "flag");
    assert_eq!(auth_status_json["auth_override_active"], true);
    assert_eq!(auth_status_json["base_url"], base_url);

    let plans = client
        .call_tool(
            CallToolRequestParams::new("ynab_list_plans").with_arguments(
                json!({
                    "include_accounts": false
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        )
        .await?;
    assert_ne!(plans.is_error, Some(true));
    let plans_text = first_text(&plans)?;
    let plans_json: Value = serde_json::from_str(plans_text)?;
    assert_eq!(plans_json["plans"][0]["id"], "plan-123");
    assert_eq!(plans_json["server_knowledge"], 77);

    client.cancel().await?;
    Ok(())
}

fn first_text(result: &rmcp::model::CallToolResult) -> anyhow::Result<&str> {
    result
        .content
        .first()
        .and_then(|content| content.raw.as_text())
        .map(|text| text.text.as_str())
        .ok_or_else(|| anyhow::anyhow!("expected text content in tool result"))
}

fn spawn_json_server(body: String) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut buffer = [0_u8; 4096];
            let _ = stream.read(&mut buffer);
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        }
    });
    format!("http://{addr}/")
}
