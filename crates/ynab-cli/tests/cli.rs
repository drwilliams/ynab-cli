use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::json;
use std::{
    fs,
    io::{Read, Write},
    net::TcpListener,
    sync::mpsc,
    thread,
};
use tempfile::TempDir;

#[test]
fn token_set_writes_success_envelope() {
    let temp_dir = TempDir::new().unwrap();
    fs::write(
        temp_dir.path().join("config.json"),
        json!({
            "version": 1,
            "current_profile": "default",
            "profiles": {
                "default": {
                    "base_url": "https://api.ynab.com/v1/",
                    "default_plan_id": "configured-plan"
                }
            }
        })
        .to_string(),
    )
    .unwrap();

    Command::cargo_bin("ynab")
        .unwrap()
        .env("YNAB_AGENT_CLI_HOME", temp_dir.path())
        .args([
            "--no-keyring",
            "auth",
            "token",
            "set",
            "--token",
            "test-token",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"ok\":true"));
}

#[test]
fn token_set_auto_selects_default_plan_when_missing() {
    let temp_dir = TempDir::new().unwrap();
    let base_url = spawn_json_server(
        json!({
            "data": {
                "plans": [
                    {
                        "id": "older-plan",
                        "name": "Older Plan",
                        "last_modified_on": "2025-01-01T00:00:00Z"
                    },
                    {
                        "id": "newer-plan",
                        "name": "Newer Plan",
                        "last_modified_on": "2025-02-01T00:00:00Z"
                    }
                ]
            }
        })
        .to_string(),
    );

    Command::cargo_bin("ynab")
        .unwrap()
        .env("YNAB_AGENT_CLI_HOME", temp_dir.path())
        .args([
            "--no-keyring",
            "--base-url",
            &base_url,
            "auth",
            "token",
            "set",
            "--token",
            "test-token",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"default_plan_id\":\"newer-plan\"",
        ))
        .stdout(predicate::str::contains(
            "\"default_plan_auto_selected\":true",
        ));

    let config: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(temp_dir.path().join("config.json")).unwrap())
            .unwrap();
    assert_eq!(
        config["profiles"]["default"]["default_plan_id"].as_str(),
        Some("newer-plan")
    );
}

#[test]
fn access_token_flag_is_used_without_persisting_session() {
    let temp_dir = TempDir::new().unwrap();
    let (base_url, request_rx) = spawn_json_server_with_request(
        json!({
            "data": {
                "plans": [
                    {
                        "id": "plan-1",
                        "name": "Plan",
                        "last_modified_on": "2025-01-01T00:00:00Z"
                    }
                ]
            }
        })
        .to_string(),
    );

    Command::cargo_bin("ynab")
        .unwrap()
        .env("YNAB_AGENT_CLI_HOME", temp_dir.path())
        .args([
            "--no-keyring",
            "--base-url",
            &base_url,
            "--access-token",
            "override-token",
            "plans",
            "list",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"id\":\"plan-1\""));

    let request = request_rx.recv().unwrap();
    assert!(request.contains("authorization: bearer override-token"));
    assert!(!temp_dir.path().join("secrets.json").exists());
}

#[test]
fn auth_status_reports_env_token_override() {
    let temp_dir = TempDir::new().unwrap();
    fs::write(
        temp_dir.path().join("config.json"),
        json!({
            "version": 1,
            "current_profile": "default",
            "profiles": {
                "default": {
                    "base_url": "https://api.ynab.com/v1/",
                    "default_plan_id": "configured-plan"
                }
            }
        })
        .to_string(),
    )
    .unwrap();

    Command::cargo_bin("ynab")
        .unwrap()
        .env("YNAB_AGENT_CLI_HOME", temp_dir.path())
        .env("YNAB_ACCESS_TOKEN", "env-token")
        .args(["--no-keyring", "auth", "status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"auth_source\":\"env\""))
        .stdout(predicate::str::contains("\"auth_override_active\":true"));
}

#[test]
fn output_transform_raw_and_jsonl_are_script_friendly() {
    let temp_dir = TempDir::new().unwrap();
    let base_url = spawn_json_server(
        json!({
            "data": {
                "plans": [
                    { "id": "plan-1", "name": "Plan 1" },
                    { "id": "plan-2", "name": "Plan 2" }
                ]
            }
        })
        .to_string(),
    );

    Command::cargo_bin("ynab")
        .unwrap()
        .env("YNAB_AGENT_CLI_HOME", temp_dir.path())
        .args([
            "--no-keyring",
            "--base-url",
            &base_url,
            "--access-token",
            "test-token",
            "--output",
            "jsonl",
            "--transform",
            "plans",
            "plans",
            "list",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("{\"id\":\"plan-1\""))
        .stdout(predicate::str::contains("{\"id\":\"plan-2\""));

    fs::write(
        temp_dir.path().join("config.json"),
        json!({
            "version": 1,
            "current_profile": "default",
            "profiles": {
                "default": {
                    "base_url": "https://api.ynab.com/v1/",
                    "default_plan_id": "configured-plan"
                }
            }
        })
        .to_string(),
    )
    .unwrap();

    Command::cargo_bin("ynab")
        .unwrap()
        .env("YNAB_AGENT_CLI_HOME", temp_dir.path())
        .args([
            "--no-keyring",
            "--transform",
            "profile",
            "--raw-output",
            "auth",
            "token",
            "set",
            "--token",
            "test-token",
        ])
        .assert()
        .success()
        .stdout(predicate::eq("default\n"));
}

#[test]
fn write_commands_require_confirmation_unless_yes_is_passed() {
    let temp_dir = TempDir::new().unwrap();
    fs::write(
        temp_dir.path().join("config.json"),
        json!({
            "version": 1,
            "current_profile": "default",
            "profiles": {
                "default": {
                    "base_url": "https://api.ynab.com/v1/",
                    "default_plan_id": "configured-plan"
                }
            }
        })
        .to_string(),
    )
    .unwrap();

    Command::cargo_bin("ynab")
        .unwrap()
        .env("YNAB_AGENT_CLI_HOME", temp_dir.path())
        .args(["--no-keyring", "transactions", "delete", "transaction-1"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("pass --yes to confirm"));
}

#[test]
fn bulk_transaction_input_defaults_to_stdin() {
    let temp_dir = TempDir::new().unwrap();
    fs::write(
        temp_dir.path().join("config.json"),
        json!({
            "version": 1,
            "current_profile": "default",
            "profiles": {
                "default": {
                    "base_url": "https://api.ynab.com/v1/",
                    "default_plan_id": "configured-plan"
                }
            }
        })
        .to_string(),
    )
    .unwrap();

    Command::cargo_bin("ynab")
        .unwrap()
        .env("YNAB_AGENT_CLI_HOME", temp_dir.path())
        .write_stdin(r#"[{"account_id":"account-1","date":"2026-04-01","amount":1000}]"#)
        .args(["--no-keyring", "transactions", "create-bulk", "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"dry_run\":true"))
        .stdout(predicate::str::contains("\"transactions\""));
}

#[test]
fn categories_update_requires_at_least_one_field() {
    let temp_dir = TempDir::new().unwrap();
    fs::write(
        temp_dir.path().join("config.json"),
        json!({
            "version": 1,
            "current_profile": "default",
            "profiles": {
                "default": {
                    "base_url": "https://api.ynab.com/v1/",
                    "default_plan_id": "configured-plan"
                }
            }
        })
        .to_string(),
    )
    .unwrap();

    Command::cargo_bin("ynab")
        .unwrap()
        .env("YNAB_AGENT_CLI_HOME", temp_dir.path())
        .args(["--no-keyring", "categories", "update", "category-1"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "requires at least one field to change",
        ));
}

#[test]
fn transactions_search_requires_at_least_one_search_term() {
    let temp_dir = TempDir::new().unwrap();
    fs::write(
        temp_dir.path().join("config.json"),
        json!({
            "version": 1,
            "current_profile": "default",
            "profiles": {
                "default": {
                    "base_url": "https://api.ynab.com/v1/",
                    "default_plan_id": "configured-plan"
                }
            }
        })
        .to_string(),
    )
    .unwrap();

    Command::cargo_bin("ynab")
        .unwrap()
        .env("YNAB_AGENT_CLI_HOME", temp_dir.path())
        .args(["--no-keyring", "transactions", "search"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "transactions search requires at least one",
        ));
}

#[test]
fn mcp_print_config_outputs_codex_and_workspace_snippets() {
    let temp_dir = TempDir::new().unwrap();
    let project_dir = temp_dir.path().join("project");
    fs::create_dir_all(&project_dir).unwrap();

    let output = Command::cargo_bin("ynab")
        .unwrap()
        .env("YNAB_AGENT_CLI_HOME", temp_dir.path())
        .args([
            "--no-keyring",
            "mcp",
            "print-config",
            "--project",
            project_dir.to_str().unwrap(),
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    let expected_project = project_dir.to_string_lossy().into_owned();
    assert_eq!(value["data"]["server_name"].as_str(), Some("ynab"));
    assert!(value["data"]["codex_config_toml"].is_string());
    assert!(value["data"]["workspace_mcp_json"].is_string());
    assert_eq!(
        value["data"]["project"].as_str(),
        Some(expected_project.as_str())
    );
}

#[test]
fn mcp_doctor_reports_auth_and_project_state() {
    let temp_dir = TempDir::new().unwrap();
    let project_dir = temp_dir.path().join("project");
    fs::create_dir_all(&project_dir).unwrap();
    fs::write(project_dir.join(".mcp.json"), "{}").unwrap();

    Command::cargo_bin("ynab")
        .unwrap()
        .env("YNAB_AGENT_CLI_HOME", temp_dir.path())
        .args([
            "--no-keyring",
            "mcp",
            "doctor",
            "--project",
            project_dir.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"binary\""))
        .stdout(predicate::str::contains("\"auth_source\":\"none\""))
        .stdout(predicate::str::contains("\"mcp_json_exists\":true"))
        .stdout(predicate::str::contains("\"summary\""));
}

#[test]
fn skill_install_codex_writes_user_skill_bundle() {
    let temp_dir = TempDir::new().unwrap();
    let home_dir = temp_dir.path().join("home");
    fs::create_dir_all(&home_dir).unwrap();

    Command::cargo_bin("ynab")
        .unwrap()
        .env("HOME", &home_dir)
        .env("YNAB_AGENT_CLI_HOME", temp_dir.path().join("runtime"))
        .args(["--no-keyring", "skill", "install", "codex"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"target\":\"codex\""))
        .stdout(predicate::str::contains("\"scope\":\"user\""));

    assert!(home_dir.join(".codex/skills/ynab-cli/SKILL.md").is_file());
    assert!(
        home_dir
            .join(".codex/skills/ynab-cli/agents/openai.yaml")
            .is_file()
    );
}

#[test]
fn skill_install_openclaw_project_writes_workspace_skill_bundle() {
    let temp_dir = TempDir::new().unwrap();
    let home_dir = temp_dir.path().join("home");
    let project_dir = temp_dir.path().join("workspace");
    fs::create_dir_all(&home_dir).unwrap();
    fs::create_dir_all(&project_dir).unwrap();

    Command::cargo_bin("ynab")
        .unwrap()
        .env("HOME", &home_dir)
        .env("YNAB_AGENT_CLI_HOME", temp_dir.path().join("runtime"))
        .args([
            "--no-keyring",
            "skill",
            "install",
            "openclaw",
            "--project",
            project_dir.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"target\":\"openclaw\""))
        .stdout(predicate::str::contains("\"scope\":\"project\""));

    assert!(project_dir.join("skills/ynab-cli/SKILL.md").is_file());
    assert!(
        project_dir
            .join("skills/ynab-cli/agents/openai.yaml")
            .is_file()
    );
}

#[test]
fn skill_install_codex_project_is_rejected() {
    let temp_dir = TempDir::new().unwrap();
    let home_dir = temp_dir.path().join("home");
    let project_dir = temp_dir.path().join("project");
    fs::create_dir_all(&home_dir).unwrap();
    fs::create_dir_all(&project_dir).unwrap();

    Command::cargo_bin("ynab")
        .unwrap()
        .env("HOME", &home_dir)
        .env("YNAB_AGENT_CLI_HOME", temp_dir.path().join("runtime"))
        .args([
            "--no-keyring",
            "skill",
            "install",
            "codex",
            "--project",
            project_dir.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "project-scoped Codex skill installs are not currently supported",
        ));
}

#[test]
fn skill_status_reports_project_support_by_target() {
    let temp_dir = TempDir::new().unwrap();
    let home_dir = temp_dir.path().join("home");
    let project_dir = temp_dir.path().join("project");
    fs::create_dir_all(&home_dir).unwrap();
    fs::create_dir_all(&project_dir).unwrap();

    Command::cargo_bin("ynab")
        .unwrap()
        .env("HOME", &home_dir)
        .env("YNAB_AGENT_CLI_HOME", temp_dir.path().join("runtime"))
        .args([
            "--no-keyring",
            "skill",
            "status",
            "--project",
            project_dir.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"skill_name\":\"ynab-cli\""))
        .stdout(predicate::str::contains("\"target\":\"codex\""))
        .stdout(predicate::str::contains("\"supported\":false"))
        .stdout(predicate::str::contains("\"target\":\"claude\""))
        .stdout(predicate::str::contains("\"target\":\"openclaw\""));
}

fn spawn_json_server(body: String) -> String {
    let (base_url, _) = spawn_json_server_with_request(body);
    base_url
}

fn spawn_json_server_with_request(body: String) -> (String, mpsc::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buffer = [0u8; 4096];
        let bytes_read = stream.read(&mut buffer).unwrap_or(0);
        let request = String::from_utf8_lossy(&buffer[..bytes_read]).to_lowercase();
        let _ = tx.send(request);
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        stream.write_all(response.as_bytes()).unwrap();
    });

    (format!("http://{address}/v1"), rx)
}
