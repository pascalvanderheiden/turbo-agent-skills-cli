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

#[test]
fn source_metadata_commands_work_without_gh_and_preserve_existing_state() {
    let directory = test_dir();
    fs::create_dir_all(&directory).unwrap();
    let state_path = directory.join("state.json");
    let invoke = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_tasc"))
            .args(args)
            .env("PATH", "")
            .env("HOME", directory.join("home"))
            .env("TASC_STATE_PATH", &state_path)
            .current_dir(&directory)
            .output()
            .unwrap()
    };
    let dry_run = invoke(&["source", "add", "ALPHA/Skills", "--dry-run", "--json"]);
    assert!(dry_run.status.success());
    assert!(!state_path.exists());
    let result: serde_json::Value = serde_json::from_slice(&dry_run.stdout).unwrap();
    assert_eq!(result["schemaVersion"], 1);
    assert_eq!(result["command"], "source add");
    assert_eq!(result["result"]["dryRun"], true);
    assert_eq!(result["warnings"], serde_json::json!([]));
    let unapproved = invoke(&["source", "add", "alpha/skills", "--json"]);
    assert_eq!(unapproved.status.code(), Some(2));
    assert!(unapproved.stdout.is_empty());
    assert!(!state_path.exists());
    let diagnostic: serde_json::Value = serde_json::from_slice(&unapproved.stderr).unwrap();
    assert_eq!(diagnostic["diagnostic"]["code"], "confirmation_required");
    assert!(
        invoke(&["--offline", "source", "add", "alpha/skills", "--yes"])
            .status
            .success()
    );
    let before = fs::read(&state_path).unwrap();
    let list = invoke(&["source", "list", "--format", "json"]);
    let list: serde_json::Value = serde_json::from_slice(&list.stdout).unwrap();
    assert_eq!(
        list["result"]["sources"][0]["id"],
        "source:https://github.com/alpha/skills"
    );
    let show = invoke(&["source", "show", "alpha/skills"]);
    let show = String::from_utf8(show.stdout).unwrap();
    assert!(show.contains("source show (schema version 1)"));
    assert!(show.contains("https://github.com/alpha/skills"));
    assert!(invoke(&["state", "status", "--offline", "--json"])
        .status
        .success());
    assert_eq!(fs::read(&state_path).unwrap(), before);

    let missing = invoke(&["search", "review", "--json"]);
    assert_eq!(missing.status.code(), Some(3));
    let diagnostic: serde_json::Value = serde_json::from_slice(&missing.stderr).unwrap();
    assert_eq!(diagnostic["diagnostic"]["code"], "gh_missing");
    let offline = invoke(&["--offline", "search", "review", "--json"]);
    assert_eq!(offline.status.code(), Some(2));
    let diagnostic: serde_json::Value = serde_json::from_slice(&offline.stderr).unwrap();
    assert_eq!(diagnostic["diagnostic"]["code"], "offline");
    assert_eq!(fs::read(&state_path).unwrap(), before);
    fs::remove_dir_all(directory).unwrap();
}

#[cfg(unix)]
#[test]
fn discovery_commands_have_stable_envelopes_and_preview_never_installs() {
    use std::os::unix::fs::PermissionsExt;
    let directory = test_dir();
    fs::create_dir_all(&directory).unwrap();
    let gh = directory.join("gh");
    fs::write(&gh, include_str!("../../tasc-agent/tests/fixtures/gh.sh")).unwrap();
    fs::set_permissions(&gh, fs::Permissions::from_mode(0o700)).unwrap();
    fs::write(directory.join("gh.mode"), "supported\n").unwrap();
    fs::write(
        directory.join("gh.search.json"),
        include_str!("../../tasc-agent/tests/fixtures/search.json"),
    )
    .unwrap();
    let state_path = directory.join("absent-state.json");
    let invoke = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_tasc"))
            .args(args)
            .env("PATH", &directory)
            .env("GH_HOST", "enterprise.example.com")
            .env("HOME", directory.join("home"))
            .env("TASC_STATE_PATH", &state_path)
            .current_dir(&directory)
            .output()
            .unwrap()
    };
    let search = invoke(&["skill", "search", "review", "--json"]);
    assert!(
        search.status.success(),
        "{}",
        String::from_utf8_lossy(&search.stderr)
    );
    assert!(search.stderr.is_empty());
    let search: serde_json::Value = serde_json::from_slice(&search.stdout).unwrap();
    assert_eq!(search["schemaVersion"], 1);
    assert_eq!(search["command"], "skill search");
    assert_eq!(
        search["result"]["items"][0]["skill"]["repo"],
        "alpha/skills"
    );
    let identity = search["result"]["items"][0]["id"].as_str().unwrap();
    for alias in ["search", "discover"] {
        let output = invoke(&[alias, "review", "--format", "json"]);
        assert!(output.status.success());
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap(),
            search
        );
    }
    let human = invoke(&["search", "review"]);
    let human = String::from_utf8(human.stdout).unwrap();
    assert!(human.contains("exact name match"));
    assert!(human.contains(identity));
    let ambiguous = invoke(&["skill", "preview", "review", "--json"]);
    assert_eq!(ambiguous.status.code(), Some(2));
    assert!(ambiguous.stdout.is_empty());
    let error: serde_json::Value = serde_json::from_slice(&ambiguous.stderr).unwrap();
    assert_eq!(error["diagnostic"]["code"], "ambiguous_selection");
    assert_eq!(
        error["diagnostic"]["candidates"].as_array().unwrap().len(),
        2
    );
    let preview = invoke(&["skill", "preview", identity, "--json"]);
    assert!(preview.status.success());
    let preview: serde_json::Value = serde_json::from_slice(&preview.stdout).unwrap();
    assert_eq!(preview["command"], "skill preview");
    assert_eq!(preview["result"]["path"], "skills/review/SKILL.md");
    assert!(preview["result"]["renderedText"]
        .as_str()
        .unwrap()
        .contains("not executed"));
    let capabilities = invoke(&["source", "capabilities", "--json"]);
    assert!(capabilities.status.success());
    let capabilities: serde_json::Value = serde_json::from_slice(&capabilities.stdout).unwrap();
    assert_eq!(capabilities["result"]["search"], true);
    assert_eq!(capabilities["result"]["preview"], true);
    fs::write(directory.join("gh.mode"), "auth\n").unwrap();
    let auth = invoke(&["search", "review", "--json"]);
    assert_eq!(auth.status.code(), Some(3));
    let auth: serde_json::Value = serde_json::from_slice(&auth.stderr).unwrap();
    assert_eq!(auth["diagnostic"]["origin"], "gh");
    assert_eq!(auth["diagnostic"]["code"], "gh_authentication");
    assert!(!state_path.exists());
    let calls = fs::read_to_string(directory.join("gh.calls")).unwrap();
    assert!(!calls.contains("install"));
    assert!(!calls.contains("publish"));
    fs::remove_dir_all(directory).unwrap();
}
