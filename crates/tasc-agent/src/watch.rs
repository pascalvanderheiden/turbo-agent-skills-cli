use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
};
use tasc_core::{
    discovery::{Diagnostic, SkillSource},
    LocalState, SourceRevision, WatchCheck, WatchCheckStatus, Watchlist, LOCAL_STATE_VERSION,
};

use crate::{
    discovery::normalize_source,
    gh::{Cancellation, GhSkillGateway},
    service::StateRepository,
};

pub trait WatchCheckRepository {
    fn save(&self, check: &WatchCheck) -> Result<(), String>;

    fn list(&self) -> Result<Vec<WatchCheck>, String> {
        Ok(Vec::new())
    }
}

#[derive(Debug, Clone)]
pub struct FsWatchCheckRepository {
    directory: PathBuf,
}

impl FsWatchCheckRepository {
    pub fn new(directory: PathBuf) -> Self {
        Self { directory }
    }
}

impl WatchCheckRepository for FsWatchCheckRepository {
    fn save(&self, check: &WatchCheck) -> Result<(), String> {
        fs::create_dir_all(&self.directory)
            .map_err(|error| format!("cannot create Watch Check directory: {error}"))?;
        let path = self
            .directory
            .join(format!("{:016x}.json", stable_hash(&check.id)));
        let contents = serde_json::to_vec_pretty(check)
            .map_err(|error| format!("cannot serialize Watch Check: {error}"))?;
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                if let Err(error) = file.write_all(&contents).and_then(|_| file.sync_all()) {
                    let _ = fs::remove_file(path);
                    return Err(format!("cannot write Watch Check: {error}"));
                }
                Ok(())
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                let existing = fs::read(path)
                    .map_err(|error| format!("cannot read existing Watch Check: {error}"))?;
                if existing == contents {
                    Ok(())
                } else {
                    Err("Watch Check reference conflicts with existing evidence".to_string())
                }
            }
            Err(error) => Err(format!("cannot create Watch Check: {error}")),
        }
    }

    fn list(&self) -> Result<Vec<WatchCheck>, String> {
        let entries = match fs::read_dir(&self.directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(format!("cannot list Watch Check artifacts: {error}")),
        };
        let mut checks = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|error| format!("cannot read Watch Check entry: {error}"))?;
            let path = entry.path();
            if path.extension().is_none_or(|extension| extension != "json") {
                continue;
            }
            let metadata = fs::symlink_metadata(&path)
                .map_err(|error| format!("cannot inspect Watch Check artifact: {error}"))?;
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err("Watch Check artifacts must be regular, non-symlink files".to_string());
            }
            let check: WatchCheck = serde_json::from_slice(
                &fs::read(&path)
                    .map_err(|error| format!("cannot read Watch Check artifact: {error}"))?,
            )
            .map_err(|error| format!("invalid Watch Check artifact: {error}"))?;
            let expected_stem = format!("{:016x}", stable_hash(&check.id));
            if path.file_stem().and_then(|stem| stem.to_str()) != Some(expected_stem.as_str()) {
                return Err("Watch Check artifact ID does not match its filename".to_string());
            }
            checks.push(check);
        }
        checks.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(checks)
    }
}

pub struct WatchService<G, R, A> {
    gateway: G,
    state_repository: R,
    artifact_repository: A,
}

impl<G: GhSkillGateway, R: StateRepository, A: WatchCheckRepository> WatchService<G, R, A> {
    pub fn new(gateway: G, state_repository: R, artifact_repository: A) -> Self {
        Self {
            gateway,
            state_repository,
            artifact_repository,
        }
    }

    pub fn list(&self) -> Result<Vec<Watchlist>, Diagnostic> {
        let mut watchlists = self.state()?.watchlists;
        watchlists.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(watchlists)
    }

    pub fn add(
        &self,
        locator: &str,
        path: Option<&str>,
        dry_run: bool,
        approved: bool,
    ) -> Result<Watchlist, Diagnostic> {
        let source = normalize_source(locator)?;
        if let Some(path) = path {
            validate_watch_path(path)?;
        }
        let id = watchlist_id(&source, path);
        let mut state = self.state()?;
        if let Some(existing) = state.watchlists.iter().find(|watchlist| watchlist.id == id) {
            return Ok(existing.clone());
        }
        let watchlist = Watchlist {
            id,
            source,
            path: path.map(str::to_string),
            last_observed_revision: None,
            last_check_id: None,
        };
        if dry_run {
            return Ok(watchlist);
        }
        if !approved {
            return Err(usage(
                "confirmation_required",
                "Adding a Watchlist target requires approval.",
                "Pass --yes or inspect with --dry-run.",
            ));
        }
        state.watchlists.push(watchlist.clone());
        state
            .watchlists
            .sort_by(|left, right| left.id.cmp(&right.id));
        self.state_repository.save(&state).map_err(state_error)?;
        Ok(watchlist)
    }

    pub fn remove(
        &self,
        selector: &str,
        dry_run: bool,
        approved: bool,
    ) -> Result<String, Diagnostic> {
        let mut state = self.state()?;
        let watchlist_id = self.resolve_one(&state.watchlists, selector)?.id.clone();
        if dry_run {
            return Ok(watchlist_id);
        }
        if !approved {
            return Err(usage(
                "confirmation_required",
                "Removing a Watchlist target requires approval.",
                "Pass --yes or inspect with --dry-run.",
            ));
        }
        state
            .watchlists
            .retain(|watchlist| watchlist.id != watchlist_id);
        self.state_repository.save(&state).map_err(state_error)?;
        Ok(watchlist_id)
    }

    pub async fn check(
        &self,
        selector: Option<&str>,
        cancel: &Cancellation,
    ) -> Result<Vec<WatchCheck>, Diagnostic> {
        let mut state = self.state()?;
        let mut selected = self
            .select(&state.watchlists, selector)?
            .into_iter()
            .cloned()
            .collect::<Vec<_>>();
        selected.sort_by(|left, right| left.id.cmp(&right.id));
        let mut checks = Vec::with_capacity(selected.len());

        for watchlist in selected {
            let prior_revision = watchlist.last_observed_revision.clone();
            let observation = self
                .gateway
                .revision(&watchlist.source, watchlist.path.as_deref(), cancel)
                .await;
            if let Err(error) = &observation {
                if error.code == "cancelled" {
                    return Err(error.clone());
                }
            }
            let (observed_revision, status, unavailable_reason) = match observation {
                Ok(Some(revision)) => {
                    let status = match &prior_revision {
                        None => WatchCheckStatus::Baseline,
                        Some(prior) if prior == &revision => WatchCheckStatus::Unchanged,
                        Some(_) => WatchCheckStatus::Changed,
                    };
                    (Some(revision), status, None)
                }
                Ok(None) => (
                    None,
                    WatchCheckStatus::Unavailable,
                    Some("source_unavailable".to_string()),
                ),
                Err(error) => (
                    None,
                    WatchCheckStatus::Unavailable,
                    Some(if error.code == "gh_not_found" {
                        "source_unavailable".to_string()
                    } else {
                        error.code
                    }),
                ),
            };
            let id = check_id(
                &watchlist.id,
                prior_revision.as_ref(),
                observed_revision.as_ref(),
                &status,
                unavailable_reason.as_deref(),
            );
            let check = WatchCheck {
                id: id.clone(),
                watchlist_id: watchlist.id.clone(),
                source: watchlist.source.clone(),
                path: watchlist.path.clone(),
                prior_revision,
                observed_revision: observed_revision.clone(),
                status,
                unavailable_reason,
            };
            self.artifact_repository.save(&check).map_err(state_error)?;

            if let Some(entry) = state
                .watchlists
                .iter_mut()
                .find(|entry| entry.id == watchlist.id)
            {
                if let Some(revision) = observed_revision {
                    entry.last_observed_revision = Some(revision);
                }
                entry.last_check_id = Some(id);
            }
            checks.push(check);
        }

        self.state_repository.save(&state).map_err(state_error)?;
        Ok(checks)
    }

    fn state(&self) -> Result<LocalState, Diagnostic> {
        let state = self.state_repository.load().map_err(state_error)?;
        if state.version != LOCAL_STATE_VERSION {
            return Err(state_error(format!(
                "unsupported Local State version {}",
                state.version
            )));
        }
        Ok(state)
    }

    fn resolve_one<'a>(
        &self,
        watchlists: &'a [Watchlist],
        selector: &str,
    ) -> Result<&'a Watchlist, Diagnostic> {
        let selected = self.select(watchlists, Some(selector))?;
        if selected.len() == 1 {
            return Ok(selected[0]);
        }
        Err(ambiguous(selected))
    }

    fn select<'a>(
        &self,
        watchlists: &'a [Watchlist],
        selector: Option<&str>,
    ) -> Result<Vec<&'a Watchlist>, Diagnostic> {
        let Some(selector) = selector else {
            return Ok(watchlists.iter().collect());
        };
        if let Some(watchlist) = watchlists.iter().find(|watchlist| watchlist.id == selector) {
            return Ok(vec![watchlist]);
        }
        let source = normalize_source(selector)?;
        let matches = watchlists
            .iter()
            .filter(|watchlist| watchlist.source.id == source.id)
            .collect::<Vec<_>>();
        if matches.is_empty() {
            return Err(usage(
                "watch_not_found",
                "No Watchlist target matches the selector.",
                "Use a Watchlist ID from `tasc watch list`.",
            ));
        }
        if matches.len() > 1 {
            return Err(ambiguous(matches));
        }
        Ok(matches)
    }
}

fn watchlist_id(source: &SkillSource, path: Option<&str>) -> String {
    match path {
        Some(path) => format!("watchlist:{}:{path}", source.id),
        None => format!("watchlist:{}", source.id),
    }
}

fn check_id(
    watchlist_id: &str,
    prior: Option<&SourceRevision>,
    observed: Option<&SourceRevision>,
    status: &WatchCheckStatus,
    unavailable_reason: Option<&str>,
) -> String {
    format!(
        "watch-check:{watchlist_id}:{}:{}:{}:{}",
        revision_key(prior),
        revision_key(observed),
        status_key(status),
        unavailable_reason.unwrap_or_default()
    )
}

fn revision_key(revision: Option<&SourceRevision>) -> String {
    revision
        .map(|revision| format!("{}-{}", revision.kind, revision.value))
        .unwrap_or_else(|| "none".to_string())
}

fn status_key(status: &WatchCheckStatus) -> &'static str {
    match status {
        WatchCheckStatus::Baseline => "baseline",
        WatchCheckStatus::Changed => "changed",
        WatchCheckStatus::Unchanged => "unchanged",
        WatchCheckStatus::Unavailable => "unavailable",
    }
}

fn validate_watch_path(path: &str) -> Result<(), Diagnostic> {
    if path.is_empty()
        || path.starts_with('/')
        || path.split('/').any(|part| {
            part.is_empty()
                || matches!(part, "." | "..")
                || part
                    .chars()
                    .any(|character| character.is_control() || matches!(character, '\\' | '@'))
        })
    {
        return Err(usage(
            "invalid_watch_path",
            "Watch path must be a safe source-relative path.",
            "Use a path without empty, dot, or parent segments.",
        ));
    }
    Ok(())
}

fn ambiguous(watchlists: Vec<&Watchlist>) -> Diagnostic {
    let mut error = usage(
        "ambiguous_watch_target",
        "Selector matches multiple Watchlist targets.",
        "Use an exact Watchlist ID.",
    );
    error.candidates = watchlists
        .into_iter()
        .map(|watchlist| watchlist.id.clone())
        .collect();
    error.candidates.sort();
    error
}

fn usage(code: &str, message: &str, remediation: &str) -> Diagnostic {
    Diagnostic::new(code, "usage", "tasc", message, remediation)
}

fn state_error(message: String) -> Diagnostic {
    Diagnostic::new(
        "local_state_failed",
        "operational",
        "filesystem",
        &message,
        "Inspect Local State and Watch Check artifacts.",
    )
}

fn stable_hash(text: &str) -> u64 {
    text.bytes().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::StateRepository;
    use std::{cell::RefCell, rc::Rc};
    use tasc_core::discovery::{Capabilities, SearchHit};

    #[derive(Clone, Default)]
    struct MemoryState(Rc<RefCell<LocalState>>);

    impl StateRepository for MemoryState {
        fn load(&self) -> Result<LocalState, String> {
            Ok(self.0.borrow().clone())
        }

        fn save(&self, state: &LocalState) -> Result<(), String> {
            *self.0.borrow_mut() = state.clone();
            Ok(())
        }
    }

    #[derive(Clone, Default)]
    struct MemoryArtifacts(Rc<RefCell<Vec<WatchCheck>>>);

    impl WatchCheckRepository for MemoryArtifacts {
        fn save(&self, check: &WatchCheck) -> Result<(), String> {
            let mut checks = self.0.borrow_mut();
            if let Some(existing) = checks.iter().find(|existing| existing.id == check.id) {
                if existing != check {
                    return Err("conflicting immutable check".into());
                }
            } else {
                checks.push(check.clone());
            }
            Ok(())
        }
    }

    struct RevisionGateway(Option<SourceRevision>);

    impl GhSkillGateway for RevisionGateway {
        async fn capabilities(&self, _: &Cancellation) -> Result<Capabilities, Diagnostic> {
            unreachable!()
        }

        async fn search(&self, _: &str, _: &Cancellation) -> Result<Vec<SearchHit>, Diagnostic> {
            unreachable!()
        }

        async fn preview(&self, _: &str, _: &str, _: &Cancellation) -> Result<String, Diagnostic> {
            unreachable!()
        }

        async fn revision(
            &self,
            _: &SkillSource,
            _: Option<&str>,
            _: &Cancellation,
        ) -> Result<Option<SourceRevision>, Diagnostic> {
            Ok(self.0.clone())
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn checks_record_transitions_and_repeat_unchanged_observations_idempotently() {
        let state = MemoryState::default();
        let artifacts = MemoryArtifacts::default();
        let service = WatchService::new(
            RevisionGateway(Some(SourceRevision::commit("a".repeat(40)))),
            state.clone(),
            artifacts.clone(),
        );
        let watch = service
            .add("owner/repo", Some("skills/example"), false, true)
            .unwrap();
        let first = service
            .check(Some(&watch.id), &Cancellation::default())
            .await
            .unwrap()
            .remove(0);
        assert_eq!(first.status, WatchCheckStatus::Baseline);
        assert_eq!(first.prior_revision, None);
        assert_eq!(first.observed_revision.unwrap().value, "a".repeat(40));

        let unchanged = service
            .check(Some(&watch.id), &Cancellation::default())
            .await
            .unwrap()
            .remove(0);
        let repeated = service
            .check(Some(&watch.id), &Cancellation::default())
            .await
            .unwrap()
            .remove(0);
        assert_eq!(unchanged.status, WatchCheckStatus::Unchanged);
        assert_eq!(unchanged.prior_revision, unchanged.observed_revision);
        assert_eq!(unchanged.id, repeated.id);
        assert_eq!(artifacts.0.borrow().len(), 2);
        assert!(state.0.borrow().installations.is_empty());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn missing_sources_and_ambiguous_source_selectors_are_explicit() {
        let state = MemoryState::default();
        let service = WatchService::new(RevisionGateway(None), state, MemoryArtifacts::default());
        let source = service.add("owner/repo", None, false, true).unwrap();
        service
            .add("owner/repo", Some("skills/a"), false, true)
            .unwrap();
        let error = service.remove("owner/repo", false, true).unwrap_err();
        assert_eq!(error.code, "ambiguous_watch_target");
        assert_eq!(error.candidates.len(), 2);
        let checks = service
            .check(Some(&source.id), &Cancellation::default())
            .await
            .unwrap();
        assert_eq!(checks[0].status, WatchCheckStatus::Unavailable);
        assert_eq!(
            checks[0].unavailable_reason.as_deref(),
            Some("source_unavailable")
        );
    }

    #[test]
    fn filesystem_watch_artifacts_can_be_listed_for_staleness_checks() {
        let directory = std::env::temp_dir().join(format!(
            "tasc-watch-artifacts-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let repository = FsWatchCheckRepository::new(directory.clone());
        let check = WatchCheck {
            id: "watch-test".to_string(),
            watchlist_id: "watchlist".to_string(),
            source: SkillSource {
                id: "source".to_string(),
                locator: "https://github.com/owner/repo".to_string(),
            },
            path: Some("skills/example".to_string()),
            prior_revision: None,
            observed_revision: Some(SourceRevision::commit("revision".to_string())),
            status: WatchCheckStatus::Baseline,
            unavailable_reason: None,
        };
        repository.save(&check).unwrap();
        assert_eq!(repository.list().unwrap(), vec![check]);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
