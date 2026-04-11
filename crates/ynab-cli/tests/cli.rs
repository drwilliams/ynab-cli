use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

#[test]
fn token_set_writes_success_envelope() {
    let temp_dir = TempDir::new().unwrap();
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
