use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
    time::{SystemTime, UNIX_EPOCH},
};

fn test_dir() -> PathBuf {
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("tasc-cli-test-{}-{time}", std::process::id()))
}

fn run(directory: &PathBuf, state_path: &PathBuf, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tasc"))
        .args(args)
        .current_dir(directory)
        .env("HOME", directory.join("home"))
        .env("TASC_STATE_PATH", state_path)
        .output()
        .unwrap()
}

#[test]
fn skill_inventory_adoption_and_state_status_share_the_same_facts() {
    let directory = test_dir();
    let skill_dir = directory.join(".github/skills/review");
    let state_path = directory.join("state/state.json");
    fs::create_dir_all(&skill_dir).unwrap();
    let skill_contents = "---\nname: Code Review\ndescription: secret-test-value\n---\n";
    fs::write(skill_dir.join("SKILL.md"), skill_contents).unwrap();

    let output = run(&directory, &state_path, &["skill", "list", "--json"]);
    assert!(output.status.success());
    let inventory: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let item = &inventory["items"][0];
    assert_eq!(item["identity"]["name"], "Code Review");
    assert_eq!(item["tracked"], false);
    assert_eq!(inventory["schema_version"], 1);
    let skill_id = item["identity"]["id"].as_str().unwrap();

    let adopted = run(&directory, &state_path, &["skill", "adopt", skill_id]);
    assert!(adopted.status.success());
    assert!(adopted.stdout.starts_with(b"Adopted "));
    assert!(!fs::read_to_string(&state_path)
        .unwrap()
        .contains("secret-test-value"));
    assert_eq!(
        fs::read_to_string(skill_dir.join("SKILL.md")).unwrap(),
        skill_contents
    );

    let output = run(&directory, &state_path, &["skill", "list", "--json"]);
    let inventory: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(inventory["items"][0]["tracked"], true);

    let output = run(&directory, &state_path, &["state", "status", "--json"]);
    assert!(output.status.success());
    let status: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(status["state_version"], 1);
    assert_eq!(status["identity_count"], 1);
    assert_eq!(status["target_count"], 1);
    assert_eq!(status["installation_count"], 1);
    assert_eq!(status["discovered_count"], 1);
    assert_eq!(status["untracked_count"], 0);

    let human_list = run(&directory, &state_path, &["skill", "list"]);
    let human_list = String::from_utf8(human_list.stdout).unwrap();
    assert!(human_list.contains("Code Review [tracked]"));
    let item = &inventory["items"][0];
    for fact in [
        item["identity"]["id"].as_str().unwrap(),
        item["identity"]["source"].as_str().unwrap(),
        item["identity"]["relative_path"].as_str().unwrap(),
        item["installation_id"].as_str().unwrap(),
        item["target"]["assistant"].as_str().unwrap(),
        item["target"]["id"].as_str().unwrap(),
        item["target"]["path"].as_str().unwrap(),
        item["path"].as_str().unwrap(),
    ] {
        assert!(
            human_list.contains(fact),
            "human inventory omitted {fact:?}"
        );
    }
    assert_eq!(item["identity"]["id"].as_str().unwrap(), skill_id);

    let human_status = run(&directory, &state_path, &["state", "status"]);
    let human_status = String::from_utf8(human_status.stdout).unwrap();
    for fact in [
        "Local State version: 1",
        "Identities: 1",
        "Adapter targets: 1",
        "Installations: 1",
        "Discovered skills: 1",
        "Untracked installations: 0",
    ] {
        assert!(human_status.contains(fact), "missing {fact:?}");
    }

    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn missing_state_is_reported_as_empty_and_future_versions_are_not_overwritten() {
    let directory = test_dir();
    fs::create_dir_all(directory.join(".github/skills")).unwrap();
    let state_path = directory.join("state/state.json");

    let output = run(&directory, &state_path, &["state", "status", "--json"]);
    assert!(output.status.success());
    let status: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(status["installation_count"], 0);

    fs::create_dir_all(state_path.parent().unwrap()).unwrap();
    let future_state = r#"{"version":99,"identities":[],"targets":[],"installations":[]}"#;
    fs::write(&state_path, future_state).unwrap();
    let output = run(&directory, &state_path, &["state", "status", "--json"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unsupported Local State version 99"));
    assert_eq!(fs::read_to_string(&state_path).unwrap(), future_state);
    fs::remove_dir_all(directory).unwrap();
}
