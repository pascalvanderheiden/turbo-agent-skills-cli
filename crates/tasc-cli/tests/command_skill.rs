use std::{
    fs,
    ops::Deref,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT_TEST_DIR: AtomicUsize = AtomicUsize::new(0);

struct TestDirectory(PathBuf);

impl Deref for TestDirectory {
    type Target = Path;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn test_dir() -> TestDirectory {
    loop {
        let directory = std::env::temp_dir().join(format!(
            "tasc-command-skill-test-{}-{}",
            std::process::id(),
            NEXT_TEST_DIR.fetch_add(1, Ordering::Relaxed)
        ));
        match fs::create_dir(&directory) {
            Ok(()) => return TestDirectory(directory),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => panic!("failed to create test directory: {error}"),
        }
    }
}

fn run(directory: &Path, state_path: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tasc"))
        .args(args)
        .current_dir(directory)
        .env("HOME", directory.join("home"))
        .env("TASC_STATE_PATH", state_path)
        .output()
        .unwrap()
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn assert_exit_code(output: &Output, expected: i32, context: &str) {
    assert_eq!(
        output.status.code(),
        Some(expected),
        "{context}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn setup_capabilities_and_install_are_explicit_and_first_party_install_is_tracked() {
    let directory = test_dir();
    let state_path = directory.join("state/state.json");
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/command-skill-parity.json")).unwrap();
    assert_eq!(fixture["surfaces"].as_array().unwrap().len(), 3);
    let handshake_scenario = &fixture["scenarios"][0];
    let handshake_args = handshake_scenario["cliArgs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        handshake_scenario["externalInvocation"],
        format!("tasc {}", handshake_args.join(" "))
    );
    assert!(include_str!("../../../skills/tasc-command/SKILL.md")
        .contains("Use the installed `tasc` executable for every TASC operation."));

    let setup = run(&directory, &state_path, &["setup", "--json"]);
    assert_success(&setup);
    let setup: serde_json::Value = serde_json::from_slice(&setup.stdout).unwrap();
    assert_eq!(setup["result"]["commandSkill"]["offered"], true);
    assert_eq!(setup["result"]["commandSkill"]["installed"], false);
    assert!(!directory.join(".github/skills/tasc-command").exists());
    assert!(!state_path.exists());
    let capabilities = run(&directory, &state_path, &handshake_args);
    assert_success(&capabilities);
    let capabilities: serde_json::Value = serde_json::from_slice(&capabilities.stdout).unwrap();
    assert_eq!(capabilities["schemaVersion"], 1);
    assert_eq!(capabilities["command"], "skill command capabilities");
    assert_eq!(
        capabilities["result"]["compatibility"],
        handshake_scenario["expected"]["compatibility"]
    );
    assert!(capabilities["result"]["capabilities"]
        .as_array()
        .unwrap()
        .contains(&serde_json::json!("security.policy")));

    let unselected_scenario = &fixture["scenarios"][2];
    let unselected_args = unselected_scenario["cliArgs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap())
        .collect::<Vec<_>>();
    let unselected = run(&directory, &state_path, &unselected_args);
    assert_exit_code(&unselected, 2, "unselected install");
    let diagnostic: serde_json::Value = serde_json::from_slice(&unselected.stderr).unwrap();
    assert_eq!(
        diagnostic["diagnostic"]["code"],
        unselected_scenario["expected"]["diagnosticCode"]
    );
    assert!(!directory.join(".github/skills/tasc-command").exists());

    let plan_scenario = &fixture["scenarios"][1];
    let plan_args = plan_scenario["cliArgs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        plan_scenario["externalInvocation"],
        format!("tasc {}", plan_args.join(" "))
    );
    let preview = run(&directory, &state_path, &plan_args);
    assert_success(&preview);
    let preview: serde_json::Value = serde_json::from_slice(&preview.stdout).unwrap();
    assert_eq!(
        preview["result"]["dryRun"],
        plan_scenario["expected"]["dryRun"]
    );
    assert_eq!(
        preview["result"]["plan"]["scope"],
        plan_scenario["expected"]["scope"]
    );
    assert_eq!(
        preview["result"]["plan"]["requiredApproval"],
        plan_scenario["expected"]["requiredApproval"]
    );
    assert_eq!(preview["result"]["plan"]["target"]["assistant"], "copilot");
    assert!(!directory.join(".github/skills/tasc-command").exists());
    assert!(!directory.join(".github/skills").exists());
    assert!(!state_path.exists());

    let approval_scenario = &fixture["scenarios"][3];
    let approval_args = approval_scenario["cliArgs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap())
        .collect::<Vec<_>>();
    let unapproved = run(&directory, &state_path, &approval_args);
    assert_exit_code(&unapproved, 2, "unapproved install");
    let diagnostic: serde_json::Value = serde_json::from_slice(&unapproved.stderr).unwrap();
    assert_eq!(
        diagnostic["diagnostic"]["code"],
        approval_scenario["expected"]["diagnosticCode"]
    );
    assert_eq!(
        plan_scenario["expected"]["noDurableEffects"],
        !directory.join(".github/skills/tasc-command").exists() && !state_path.exists()
    );

    let installed = run(
        &directory,
        &state_path,
        &[
            "skill",
            "command",
            "install",
            "--adapter",
            "copilot",
            "--scope",
            "project",
            "--yes",
            "--json",
        ],
    );
    assert_success(&installed);
    let installed: serde_json::Value = serde_json::from_slice(&installed.stdout).unwrap();
    assert_eq!(installed["result"]["status"], "installed");
    assert_eq!(
        installed["result"]["installation"]["source_revision"],
        env!("CARGO_PKG_VERSION")
    );
    assert_eq!(
        installed["result"]["installation"]["content_digest"],
        installed["result"]["plan"]["packageDigest"]
    );
    assert_eq!(
        installed["result"]["installation"]["content_digest"]
            .as_str()
            .unwrap()
            .len(),
        64
    );

    let inventory = run(&directory, &state_path, &["skill", "list", "--json"]);
    assert_success(&inventory);
    let inventory: serde_json::Value = serde_json::from_slice(&inventory.stdout).unwrap();
    let command_skill = &inventory["items"][0];
    assert_eq!(command_skill["tracked"], true);
    assert_eq!(command_skill["provenance"]["kind"], "bundled_first_party");
    assert_eq!(command_skill["provenance"]["confidence"], "verified");
    assert_eq!(
        command_skill["identity"]["source"],
        "tasc://first-party/command-skill"
    );

    let listed = run(
        &directory,
        &state_path,
        &["skill", "command", "list", "--json"],
    );
    assert_success(&listed);
    let listed: serde_json::Value = serde_json::from_slice(&listed.stdout).unwrap();
    assert_eq!(
        listed["result"]["installations"].as_array().unwrap().len(),
        1
    );
}

#[test]
fn repair_and_uninstall_refuse_to_overwrite_user_modifications() {
    let directory = test_dir();
    let state_path = directory.join("state/state.json");
    let skill_path = directory.join(".github/skills/tasc-command");
    let install = run(
        &directory,
        &state_path,
        &[
            "skill",
            "command",
            "install",
            "--adapter",
            "copilot",
            "--scope",
            "project",
            "--yes",
            "--json",
        ],
    );
    assert_success(&install);

    fs::write(
        skill_path.join("SKILL.md"),
        format!(
            "{}\nUser customization.\n",
            fs::read_to_string(skill_path.join("SKILL.md")).unwrap()
        ),
    )
    .unwrap();
    let customized = fs::read(skill_path.join("SKILL.md")).unwrap();

    for operation in ["update", "repair", "uninstall"] {
        let refused = run(
            &directory,
            &state_path,
            &[
                "skill",
                "command",
                operation,
                "--adapter",
                "copilot",
                "--scope",
                "project",
                "--yes",
                "--json",
            ],
        );
        assert_exit_code(&refused, 2, operation);
        let diagnostic: serde_json::Value = serde_json::from_slice(&refused.stderr).unwrap();
        assert_eq!(
            diagnostic["diagnostic"]["code"], "modified_command_skill",
            "{operation}"
        );
        assert_eq!(fs::read(skill_path.join("SKILL.md")).unwrap(), customized);
    }

    fs::remove_dir_all(&skill_path).unwrap();
    let repaired = run(
        &directory,
        &state_path,
        &[
            "skill",
            "command",
            "repair",
            "--adapter",
            "copilot",
            "--scope",
            "project",
            "--yes",
            "--json",
        ],
    );
    assert_success(&repaired);
    assert!(skill_path.join("SKILL.md").is_file());

    let uninstalled = run(
        &directory,
        &state_path,
        &[
            "skill",
            "command",
            "uninstall",
            "--adapter",
            "copilot",
            "--scope",
            "project",
            "--yes",
            "--json",
        ],
    );
    assert_success(&uninstalled);
    assert!(!skill_path.exists());
}

#[test]
fn all_supported_requires_an_explicit_opt_in_and_a_detected_adapter() {
    let directory = test_dir();
    let state_path = directory.join("state/state.json");

    let no_detection = run(
        &directory,
        &state_path,
        &[
            "skill",
            "command",
            "install",
            "--all-supported",
            "--scope",
            "project",
            "--dry-run",
            "--json",
        ],
    );
    assert_exit_code(&no_detection, 2, "install without detected adapters");
    let diagnostic: serde_json::Value = serde_json::from_slice(&no_detection.stderr).unwrap();
    assert_eq!(
        diagnostic["diagnostic"]["code"],
        "no_supported_adapters_detected"
    );

    fs::create_dir_all(directory.join(".github/skills")).unwrap();
    let detected = run(
        &directory,
        &state_path,
        &[
            "skill", "command", "adapters", "--scope", "project", "--json",
        ],
    );
    assert_success(&detected);
    let detected: serde_json::Value = serde_json::from_slice(&detected.stdout).unwrap();
    assert_eq!(detected["result"]["adapters"][0]["id"], "copilot");
    assert_eq!(detected["result"]["adapters"][0]["detected"], true);

    let all_supported = run(
        &directory,
        &state_path,
        &[
            "skill",
            "command",
            "install",
            "--all-supported",
            "--scope",
            "project",
            "--dry-run",
            "--json",
        ],
    );
    assert_success(&all_supported);
}
