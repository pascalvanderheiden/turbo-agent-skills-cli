#![cfg(unix)]

use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tasc_agent::gh::{Cancellation, GhSkill, GhSkillGateway};
use tasc_core::discovery::SkillSource;

struct Fixture(PathBuf);

impl Fixture {
    fn new(mode: &str) -> Self {
        let directory = std::env::temp_dir().join(format!(
            "tasc-gh-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&directory).unwrap();
        let fixture = Self(directory);
        fs::write(fixture.0.join("gh"), include_str!("fixtures/gh.sh")).unwrap();
        fs::set_permissions(fixture.0.join("gh"), fs::Permissions::from_mode(0o700)).unwrap();
        fs::write(fixture.0.join("gh.mode"), format!("{mode}\n")).unwrap();
        fs::write(
            fixture.0.join("gh.search.json"),
            include_str!("fixtures/search.json"),
        )
        .unwrap();
        fixture
    }

    fn gateway(&self) -> GhSkill {
        GhSkill::new(self.0.join("gh"), Duration::from_secs(2))
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[tokio::test(flavor = "current_thread")]
async fn supported_wire_contract_is_typed_noninteractive_and_probes_are_cached() {
    let fixture = Fixture::new("supported");
    let gateway = fixture.gateway();
    let cancel = Cancellation::default();
    assert_eq!(gateway.search("review", &cancel).await.unwrap().len(), 4);
    gateway
        .search("--help; touch /tmp/not-a-shell-command", &cancel)
        .await
        .unwrap();
    let capabilities = gateway.capabilities(&cancel).await.unwrap();
    assert!(capabilities.search && capabilities.preview);
    let preview = gateway
        .preview("alpha/skills", "skills/review/SKILL.md", &cancel)
        .await
        .unwrap();
    assert!(preview.starts_with("Preview only"));
    let calls = fs::read_to_string(fixture.0.join("gh.calls")).unwrap();
    assert_eq!(calls.lines().filter(|line| *line == "version").count(), 1);
    assert_eq!(
        calls
            .lines()
            .filter(|line| *line == "skill search --help")
            .count(),
        1
    );
    assert!(calls.contains("--limit 100 -- review"));
    assert!(calls.contains("skill preview -- alpha/skills skills/review/SKILL.md"));
}

#[tokio::test(flavor = "current_thread")]
async fn missing_capabilities_are_explicit_and_not_empty_successes() {
    for mode in ["unavailable", "no-search", "no-json", "bad-version"] {
        let fixture = Fixture::new(mode);
        let error = fixture
            .gateway()
            .search("review", &Cancellation::default())
            .await
            .unwrap_err();
        assert_eq!(error.code, "gh_unsupported", "{mode}");
        assert_eq!(error.exit_code(), 3);
    }
    let fixture = Fixture::new("no-preview");
    let capabilities = fixture
        .gateway()
        .capabilities(&Cancellation::default())
        .await
        .unwrap();
    assert!(capabilities.search);
    assert!(!capabilities.preview);
    // An unrelated unavailable preview must not disable search.
    assert!(fixture
        .gateway()
        .search("review", &Cancellation::default())
        .await
        .is_ok());
    let missing = GhSkill::new(fixture.0.join("missing"), Duration::from_secs(1));
    assert_eq!(
        missing
            .capabilities(&Cancellation::default())
            .await
            .unwrap_err()
            .code,
        "gh_missing"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn process_failures_and_malformed_output_have_stable_diagnostics() {
    for (mode, expected) in [
        ("auth", "gh_authentication"),
        ("auth-env", "gh_authentication"),
        ("forbidden", "gh_access_denied"),
        ("fail", "gh_failed"),
        ("malformed", "gh_malformed_output"),
        ("invalid-utf8", "gh_malformed_output"),
        ("oversized", "gh_output_limit"),
        ("oversized-stderr", "gh_output_limit"),
    ] {
        let fixture = Fixture::new(mode);
        let error = fixture
            .gateway()
            .search("review", &Cancellation::default())
            .await
            .unwrap_err();
        assert_eq!(error.code, expected, "{mode}");
    }
}

#[tokio::test(flavor = "current_thread")]
async fn timeout_and_cancellation_bound_execution_and_reap_the_child() {
    let fixture = Fixture::new("timeout");
    let gateway = GhSkill::new(fixture.0.join("gh"), Duration::from_millis(150));
    let start = Instant::now();
    let error = gateway
        .search("review", &Cancellation::default())
        .await
        .unwrap_err();
    assert_eq!(error.code, "gh_timeout");
    assert!(start.elapsed() < Duration::from_secs(2));

    let cancel = Cancellation::default();
    let gateway = fixture.gateway();
    let operation = gateway.search("review", &cancel);
    let trigger = async {
        tokio::time::sleep(Duration::from_millis(100)).await;
        cancel.cancel();
    };
    let (result, ()) = tokio::join!(operation, trigger);
    assert_eq!(result.unwrap_err().exit_code(), 130);
    assert_eq!(
        gateway.search("review", &cancel).await.unwrap_err().code,
        "cancelled"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn revision_observation_returns_exact_commits_and_explicit_unavailable_states() {
    let fixture = Fixture::new("supported");
    let source = SkillSource {
        id: "source:https://github.com/alpha/skills".into(),
        locator: "https://github.com/alpha/skills".into(),
    };
    let gateway = fixture.gateway();
    let revision = gateway
        .revision(
            &source,
            Some("skills/review/SKILL.md"),
            &Cancellation::default(),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(revision.kind, "commit");
    assert_eq!(revision.value, "0123456789abcdef0123456789abcdef01234567");
    let calls = fs::read_to_string(fixture.0.join("gh.calls")).unwrap();
    assert!(calls.contains(    "api repos/alpha/skills/commits --method GET --field per_page=1 --field path=skills/review/SKILL.md --jq"));

    let missing = Fixture::new("empty-revision");
    assert!(missing
        .gateway()
        .revision(&source, None, &Cancellation::default())
        .await
        .unwrap()
        .is_none());

    let missing = Fixture::new("source-missing");
    assert_eq!(
        missing
            .gateway()
            .revision(&source, None, &Cancellation::default())
            .await
            .unwrap_err()
            .code,
        "gh_not_found"
    );
}
