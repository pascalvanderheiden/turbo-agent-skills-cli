use crate::service::{skill_content_digest, StateRepository};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};
use tasc_core::{
    discovery::Diagnostic,
    evaluation::{
        EvaluationAnalyzer, EvaluationDiagnostic, EvaluationOutcome, EvaluationPolicy, RunArtifact,
    },
    AdapterTarget, Installation, LocalState, SkillIdentity, SourceRevision, LOCAL_STATE_VERSION,
};

static RUN_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub trait RunArtifactRepository {
    fn save(&self, artifact: &RunArtifact) -> Result<(), String>;
    fn list(&self) -> Result<Vec<RunArtifact>, String>;
    fn get(&self, id: &str) -> Result<RunArtifact, String>;
}

#[derive(Debug, Clone)]
pub struct FsRunArtifactRepository {
    directory: PathBuf,
}

impl FsRunArtifactRepository {
    pub fn new(directory: PathBuf) -> Self {
        Self { directory }
    }
}

impl RunArtifactRepository for FsRunArtifactRepository {
    fn save(&self, artifact: &RunArtifact) -> Result<(), String> {
        fs::create_dir_all(&self.directory)
            .map_err(|error| format!("cannot create Evaluation artifact directory: {error}"))?;
        let path = artifact_path(&self.directory, &artifact.id)?;
        let contents = serde_json::to_vec_pretty(artifact)
            .map_err(|error| format!("cannot serialize Evaluation artifact: {error}"))?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::AlreadyExists {
                    "Evaluation Run Artifacts are immutable; this artifact ID already exists"
                        .to_string()
                } else {
                    format!("cannot create Evaluation artifact: {error}")
                }
            })?;
        if let Err(error) = file.write_all(&contents).and_then(|_| file.sync_all()) {
            let _ = fs::remove_file(path);
            return Err(format!("cannot write Evaluation artifact: {error}"));
        }
        Ok(())
    }

    fn list(&self) -> Result<Vec<RunArtifact>, String> {
        let entries = match fs::read_dir(&self.directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(format!("cannot list Evaluation artifacts: {error}")),
        };
        let mut artifacts = Vec::new();
        for entry in entries {
            let entry =
                entry.map_err(|error| format!("cannot read Evaluation artifact entry: {error}"))?;
            if entry
                .path()
                .extension()
                .is_none_or(|extension| extension != "json")
            {
                continue;
            }
            let artifact: RunArtifact = serde_json::from_slice(
                &fs::read(entry.path())
                    .map_err(|error| format!("cannot read Evaluation artifact: {error}"))?,
            )
            .map_err(|error| format!("invalid Evaluation artifact: {error}"))?;
            if artifact_path(&self.directory, &artifact.id)? != entry.path() {
                return Err("Evaluation artifact ID does not match its filename".to_string());
            }
            artifacts.push(artifact);
        }
        artifacts.sort_by(|left, right| {
            left.created_at_unix_ms
                .cmp(&right.created_at_unix_ms)
                .then_with(|| left.id.cmp(&right.id))
        });
        Ok(artifacts)
    }

    fn get(&self, id: &str) -> Result<RunArtifact, String> {
        let path = artifact_path(&self.directory, id)?;
        let bytes = fs::read(path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                format!("no Evaluation Run Artifact has ID '{id}'")
            } else {
                format!("cannot read Evaluation artifact: {error}")
            }
        })?;
        let artifact: RunArtifact = serde_json::from_slice(&bytes)
            .map_err(|error| format!("invalid Evaluation artifact: {error}"))?;
        if artifact.id != id {
            return Err("Evaluation artifact ID does not match its filename".to_string());
        }
        Ok(artifact)
    }
}

pub struct EvaluationService<R, A> {
    state_repository: R,
    artifact_repository: A,
}

impl<R: StateRepository, A: RunArtifactRepository> EvaluationService<R, A> {
    pub fn new(state_repository: R, artifact_repository: A) -> Self {
        Self {
            state_repository,
            artifact_repository,
        }
    }

    pub fn run(
        &self,
        selector: &str,
        revision: Option<&str>,
        require_waza: bool,
    ) -> Result<RunArtifact, Diagnostic> {
        let started_at_unix_ms = now_unix_ms();
        let state = self.state_repository.load().map_err(state_error)?;
        if state.version != LOCAL_STATE_VERSION {
            return Err(state_error(format!(
                "unsupported Local State version {}; expected {}",
                state.version, LOCAL_STATE_VERSION
            )));
        }
        let selected = resolve_selection(&state, selector, revision)?;
        let mut diagnostics = Vec::new();
        let package_path = selected
            .installation
            .as_ref()
            .map(|installation| PathBuf::from(&installation.path))
            .or_else(|| local_package_path(&selected.identity));
        let package = inspect_package(package_path.as_deref());
        diagnostics.extend(package.diagnostics);
        let mut outcome = package.outcome;
        if require_waza {
            let (available, path) = executable_path("waza");
            if !available {
                diagnostics.push(EvaluationDiagnostic {
                    code: "optional_dependency_unavailable".to_string(),
                    severity: "warning".to_string(),
                    message: "Waza was requested but is not available on PATH; scenario execution was not run.".to_string(),
                    path: None,
                });
                if outcome == EvaluationOutcome::Passed {
                    outcome = EvaluationOutcome::Inconclusive;
                }
            } else {
                diagnostics.push(EvaluationDiagnostic {
                    code: "scenario_execution_not_run".to_string(),
                    severity: "info".to_string(),
                    message: format!(
                        "Waza is available at {}, but this Evaluation performs package-only smoke validation.",
                        path.unwrap_or_default().display()
                    ),
                    path: None,
                });
            }
        }
        let completed_at_unix_ms = now_unix_ms();
        let sequence = RUN_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let artifact = RunArtifact {
            schema_version: 1,
            id: format!(
                "run-{}-{}-{sequence}",
                started_at_unix_ms,
                std::process::id()
            ),
            created_at_unix_ms: started_at_unix_ms,
            completed_at_unix_ms,
            selector: selector.to_string(),
            identity: selected.identity,
            source_revision: selected.source_revision,
            target: selected.target,
            installation_id: selected.installation.map(|installation| installation.id),
            package_path: package_path.map(|path| path.to_string_lossy().into_owned()),
            package_digest: package.digest,
            analyzer: EvaluationAnalyzer {
                name: "tasc-package-smoke".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                configuration: vec![
                    "required_file=SKILL.md".to_string(),
                    "required_frontmatter=name,description".to_string(),
                    "max_skill_file_bytes=1048576".to_string(),
                ],
            },
            policy: EvaluationPolicy {
                mode: "package_only".to_string(),
                package_validation: "required".to_string(),
                scenario_execution: "not_run".to_string(),
            },
            outcome,
            package_outcome: package.outcome,
            execution_outcome: EvaluationOutcome::NotRun,
            diagnostics,
        };
        self.artifact_repository
            .save(&artifact)
            .map_err(state_error)?;
        Ok(artifact)
    }

    pub fn list(&self) -> Result<Vec<RunArtifact>, Diagnostic> {
        self.artifact_repository.list().map_err(state_error)
    }

    pub fn show(&self, id: &str) -> Result<RunArtifact, Diagnostic> {
        self.artifact_repository.get(id).map_err(state_error)
    }
}

struct Selected {
    identity: SkillIdentity,
    source_revision: Option<SourceRevision>,
    target: Option<AdapterTarget>,
    installation: Option<Installation>,
}

fn resolve_selection(
    state: &LocalState,
    selector: &str,
    revision: Option<&str>,
) -> Result<Selected, Diagnostic> {
    let matching_installations = |identity_id: Option<&str>| {
        state
            .installations
            .iter()
            .filter(|installation| {
                identity_id.is_none_or(|identity_id| installation.skill_id == identity_id)
                    && revision.is_none_or(|revision| {
                        installation.source_revision.as_deref() == Some(revision)
                    })
            })
            .collect::<Vec<_>>()
    };

    if let Some(installation) = state
        .installations
        .iter()
        .find(|installation| installation.id == selector)
    {
        return selected_installation(state, installation, revision);
    }

    if let Some(identity) = state
        .identities
        .iter()
        .find(|identity| identity.id == selector || identity.name == selector)
    {
        let installations = matching_installations(Some(&identity.id));
        return match installations.as_slice() {
            [installation] => selected_installation(state, installation, revision),
            [] if revision.is_none() => Ok(Selected {
                identity: identity.clone(),
                source_revision: None,
                target: None,
                installation: None,
            }),
            [] => Err(selection_error(
                "revision_not_found",
                &format!(
                    "No Installation of '{}' records revision '{}'.",
                    identity.name,
                    revision.unwrap()
                ),
                vec![],
            )),
            many => Err(selection_error(
                "ambiguous_selection",
                "The selected Skill Identity has multiple matching Installations.",
                many.iter()
                    .map(|installation| installation.id.clone())
                    .collect(),
            )),
        };
    }

    let installations = matching_installations(None)
        .into_iter()
        .filter(|installation| {
            installation.source_revision.as_deref() == Some(selector)
                || revision.is_some_and(|revision| {
                    installation.source_revision.as_deref() == Some(revision)
                })
        })
        .collect::<Vec<_>>();
    match installations.as_slice() {
        [installation] => selected_installation(state, installation, revision),
        [] => Err(selection_error(
            "selection_not_found",
            &format!("No Skill Identity, Source Revision, or Installation matches '{selector}'."),
            vec![],
        )),
        many => Err(selection_error(
            "ambiguous_selection",
            &format!("'{selector}' matches multiple Installations."),
            many.iter()
                .map(|installation| installation.id.clone())
                .collect(),
        )),
    }
}

fn selected_installation(
    state: &LocalState,
    installation: &Installation,
    revision: Option<&str>,
) -> Result<Selected, Diagnostic> {
    if revision.is_some_and(|revision| installation.source_revision.as_deref() != Some(revision)) {
        return Err(selection_error(
            "revision_not_found",
            &format!(
                "Installation '{}' does not record the selected revision.",
                installation.id
            ),
            vec![],
        ));
    }
    let identity = state
        .identities
        .iter()
        .find(|identity| identity.id == installation.skill_id)
        .cloned()
        .ok_or_else(|| {
            selection_error(
                "identity_not_found",
                "The selected Installation references an unknown Skill Identity.",
                vec![],
            )
        })?;
    let target = state
        .targets
        .iter()
        .find(|target| target.id == installation.target_id)
        .cloned()
        .ok_or_else(|| {
            selection_error(
                "target_not_found",
                "The selected Installation references an unknown Adapter Target.",
                vec![],
            )
        })?;
    Ok(Selected {
        identity,
        source_revision: installation
            .source_revision
            .as_ref()
            .map(|value| SourceRevision {
                kind: "recorded".to_string(),
                value: value.clone(),
            }),
        target: Some(target),
        installation: Some(installation.clone()),
    })
}

struct PackageInspection {
    outcome: EvaluationOutcome,
    digest: Option<String>,
    diagnostics: Vec<EvaluationDiagnostic>,
}

fn inspect_package(path: Option<&Path>) -> PackageInspection {
    let Some(path) = path else {
        return inconclusive(
            "package_unavailable",
            "No local package is available for this Skill Identity.",
            None,
        );
    };
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return inconclusive(
                "package_unavailable",
                "The selected package path does not exist locally.",
                Some(path),
            )
        }
        Err(error) => {
            return inconclusive(
                "package_unreadable",
                &format!("Cannot inspect the selected package: {error}"),
                Some(path),
            )
        }
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return failed(
            "invalid_package_path",
            "The selected package must be a non-symlink directory.",
            Some(path),
        );
    }
    let digest = match skill_content_digest(path) {
        Ok(digest) => Some(digest),
        Err(error) => {
            return inconclusive(
                "package_digest_unavailable",
                &format!("Cannot establish package content provenance: {error}"),
                Some(path),
            )
        }
    };
    let skill_file = path.join("SKILL.md");
    let file_metadata = match fs::symlink_metadata(&skill_file) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return PackageInspection {
                outcome: EvaluationOutcome::Failed,
                digest,
                diagnostics: vec![diagnostic(
                    "required_file_missing",
                    "error",
                    "Package is missing required file SKILL.md.",
                    Some(&skill_file),
                )],
            }
        }
        Err(error) => {
            return inconclusive(
                "package_unreadable",
                &format!("Cannot inspect SKILL.md: {error}"),
                Some(&skill_file),
            )
        }
    };
    if file_metadata.file_type().is_symlink() || !file_metadata.is_file() {
        return PackageInspection {
            outcome: EvaluationOutcome::Failed,
            digest,
            diagnostics: vec![diagnostic(
                "invalid_skill_file",
                "error",
                "SKILL.md must be a regular, non-symlink file.",
                Some(&skill_file),
            )],
        };
    }
    if file_metadata.len() > 1_048_576 {
        return inconclusive(
            "skill_file_too_large",
            "SKILL.md exceeds the 1 MiB deterministic analyzer limit.",
            Some(&skill_file),
        );
    }
    let contents = match fs::read_to_string(&skill_file) {
        Ok(contents) => contents,
        Err(error) => {
            return inconclusive(
                "package_unreadable",
                &format!("Cannot read SKILL.md as UTF-8: {error}"),
                Some(&skill_file),
            )
        }
    };
    match validate_frontmatter(&contents) {
        Ok(()) => PackageInspection {
            outcome: EvaluationOutcome::Passed,
            digest,
            diagnostics: vec![diagnostic(
                "package_smoke_passed",
                "info",
                "Package has a valid SKILL.md with non-empty name and description metadata.",
                Some(&skill_file),
            )],
        },
        Err(message) => PackageInspection {
            outcome: EvaluationOutcome::Failed,
            digest,
            diagnostics: vec![diagnostic(
                "malformed_package",
                "error",
                &message,
                Some(&skill_file),
            )],
        },
    }
}

fn validate_frontmatter(contents: &str) -> Result<(), String> {
    let mut lines = contents.lines();
    if lines.next().is_none_or(|line| line.trim() != "---") {
        return Err("SKILL.md must begin with YAML frontmatter delimited by '---'.".to_string());
    }
    let mut name = false;
    let mut description = false;
    let mut closed = false;
    for line in lines {
        if line.trim() == "---" {
            closed = true;
            break;
        }
        if let Some((key, value)) = line.split_once(':') {
            let value = value.trim().trim_matches(['\'', '"']);
            match key.trim() {
                "name" => name = !value.is_empty(),
                "description" => description = !value.is_empty(),
                _ => {}
            }
        }
    }
    if !closed {
        return Err("SKILL.md frontmatter is not closed by '---'.".to_string());
    }
    if !name {
        return Err("SKILL.md frontmatter must include a non-empty name.".to_string());
    }
    if !description {
        return Err("SKILL.md frontmatter must include a non-empty description.".to_string());
    }
    Ok(())
}

fn local_package_path(identity: &SkillIdentity) -> Option<PathBuf> {
    let source = identity.source.strip_prefix("file://")?;
    let source = decode_uri_path(source)?;
    let relative = Path::new(&identity.relative_path);
    if relative.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return None;
    }
    Some(PathBuf::from(source).join(relative))
}

fn decode_uri_path(path: &str) -> Option<String> {
    let bytes = path.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let high = (*bytes.get(index + 1)? as char).to_digit(16)?;
            let low = (*bytes.get(index + 2)? as char).to_digit(16)?;
            decoded.push((high * 16 + low) as u8);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).ok()
}

fn executable_path(name: &str) -> (bool, Option<PathBuf>) {
    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .map(|directory| directory.join(name))
        .find(|path| path.is_file())
        .map_or((false, None), |path| (true, Some(path)))
}

fn artifact_path(directory: &Path, id: &str) -> Result<PathBuf, String> {
    if id.is_empty()
        || !id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-')
    {
        return Err("invalid Evaluation artifact ID".to_string());
    }
    Ok(directory.join(format!("{id}.json")))
}

fn now_unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn diagnostic(
    code: &str,
    severity: &str,
    message: &str,
    path: Option<&Path>,
) -> EvaluationDiagnostic {
    EvaluationDiagnostic {
        code: code.to_string(),
        severity: severity.to_string(),
        message: message.to_string(),
        path: path.map(|path| path.to_string_lossy().into_owned()),
    }
}

fn failed(code: &str, message: &str, path: Option<&Path>) -> PackageInspection {
    PackageInspection {
        outcome: EvaluationOutcome::Failed,
        digest: None,
        diagnostics: vec![diagnostic(code, "error", message, path)],
    }
}

fn inconclusive(code: &str, message: &str, path: Option<&Path>) -> PackageInspection {
    PackageInspection {
        outcome: EvaluationOutcome::Inconclusive,
        digest: None,
        diagnostics: vec![diagnostic(code, "warning", message, path)],
    }
}

fn selection_error(code: &str, message: &str, candidates: Vec<String>) -> Diagnostic {
    let mut diagnostic = Diagnostic::new(
        code,
        "usage",
        "evaluation",
        message,
        "Select an unambiguous Skill Identity, Source Revision, or Installation.",
    );
    diagnostic.candidates = candidates;
    diagnostic
}

fn state_error(message: String) -> Diagnostic {
    Diagnostic::new(
        "evaluation_failed",
        "operational",
        "evaluation",
        &message,
        "Inspect Local State and Evaluation artifact storage, then retry.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::{Arc, Mutex},
        time::{SystemTime, UNIX_EPOCH},
    };

    #[derive(Clone)]
    struct MemoryState(LocalState);

    impl StateRepository for MemoryState {
        fn load(&self) -> Result<LocalState, String> {
            Ok(self.0.clone())
        }

        fn save(&self, _state: &LocalState) -> Result<(), String> {
            unreachable!()
        }
    }

    #[derive(Clone, Default)]
    struct MemoryArtifacts(Arc<Mutex<Vec<RunArtifact>>>);

    impl RunArtifactRepository for MemoryArtifacts {
        fn save(&self, artifact: &RunArtifact) -> Result<(), String> {
            let mut artifacts = self.0.lock().unwrap();
            if artifacts.iter().any(|existing| existing.id == artifact.id) {
                return Err("duplicate artifact".to_string());
            }
            artifacts.push(artifact.clone());
            Ok(())
        }

        fn list(&self) -> Result<Vec<RunArtifact>, String> {
            Ok(self.0.lock().unwrap().clone())
        }

        fn get(&self, id: &str) -> Result<RunArtifact, String> {
            self.0
                .lock()
                .unwrap()
                .iter()
                .find(|artifact| artifact.id == id)
                .cloned()
                .ok_or_else(|| "not found".to_string())
        }
    }

    fn temp_dir() -> PathBuf {
        let time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("tasc-eval-{}-{time}", std::process::id()))
    }

    fn state_with_installation(path: &Path) -> LocalState {
        let identity = SkillIdentity {
            id: "skill:demo".to_string(),
            name: "demo".to_string(),
            source: "https://github.com/demo/skills".to_string(),
            relative_path: "demo".to_string(),
        };
        let target = AdapterTarget {
            id: "target:copilot".to_string(),
            assistant: "copilot".to_string(),
            path: "/skills".to_string(),
        };
        LocalState {
            identities: vec![identity],
            targets: vec![target],
            installations: vec![Installation {
                id: "installation:demo".to_string(),
                skill_id: "skill:demo".to_string(),
                target_id: "target:copilot".to_string(),
                path: path.to_string_lossy().into_owned(),
                source_revision: Some("commit-123".to_string()),
                content_digest: None,
                status: tasc_core::InstallationStatus::Installed,
                last_error: None,
            }],
            ..LocalState::default()
        }
    }

    fn write_skill(path: &Path, contents: &str) {
        fs::create_dir_all(path).unwrap();
        fs::write(path.join("SKILL.md"), contents).unwrap();
    }

    #[test]
    fn package_smoke_records_provenance_and_not_run_execution() {
        let directory = temp_dir();
        write_skill(
            &directory,
            "---\nname: demo\ndescription: a valid skill\n---\nBody.\n",
        );
        let artifacts = MemoryArtifacts::default();
        let service = EvaluationService::new(
            MemoryState(state_with_installation(&directory)),
            artifacts.clone(),
        );

        let result = service.run("installation:demo", None, false).unwrap();
        assert_eq!(result.outcome, EvaluationOutcome::Passed);
        assert_eq!(result.package_outcome, EvaluationOutcome::Passed);
        assert_eq!(result.execution_outcome, EvaluationOutcome::NotRun);
        assert_eq!(result.identity.id, "skill:demo");
        assert_eq!(
            result.source_revision,
            Some(SourceRevision {
                kind: "recorded".to_string(),
                value: "commit-123".to_string()
            })
        );
        assert_eq!(result.target.as_ref().unwrap().id, "target:copilot");
        assert!(result.package_digest.is_some());
        assert_eq!(artifacts.list().unwrap(), vec![result]);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn malformed_and_unavailable_packages_are_not_reported_as_passed() {
        let directory = temp_dir();
        write_skill(&directory, "---\nname: demo\n---\nMissing description.\n");
        let service = EvaluationService::new(
            MemoryState(state_with_installation(&directory)),
            MemoryArtifacts::default(),
        );
        let malformed = service.run("skill:demo", None, false).unwrap();
        assert_eq!(malformed.outcome, EvaluationOutcome::Failed);
        assert_eq!(malformed.diagnostics[0].code, "malformed_package");

        let missing_path = temp_dir();
        let unavailable = EvaluationService::new(
            MemoryState(state_with_installation(&missing_path)),
            MemoryArtifacts::default(),
        )
        .run("installation:demo", None, false)
        .unwrap();
        assert_eq!(unavailable.outcome, EvaluationOutcome::Inconclusive);
        assert_eq!(unavailable.diagnostics[0].code, "package_unavailable");
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn immutable_filesystem_repository_lists_and_reads_artifacts() {
        let directory = temp_dir();
        let repository = FsRunArtifactRepository::new(directory.clone());
        let mut artifact = RunArtifact {
            schema_version: 1,
            id: "run-1".to_string(),
            created_at_unix_ms: 1,
            completed_at_unix_ms: 2,
            selector: "skill:demo".to_string(),
            identity: SkillIdentity {
                id: "skill:demo".to_string(),
                name: "demo".to_string(),
                source: "file:///skills".to_string(),
                relative_path: "demo".to_string(),
            },
            source_revision: None,
            target: None,
            installation_id: None,
            package_path: None,
            package_digest: None,
            analyzer: EvaluationAnalyzer {
                name: "tasc-package-smoke".to_string(),
                version: "0.1.0".to_string(),
                configuration: Vec::new(),
            },
            policy: EvaluationPolicy {
                mode: "package_only".to_string(),
                package_validation: "required".to_string(),
                scenario_execution: "not_run".to_string(),
            },
            outcome: EvaluationOutcome::Inconclusive,
            package_outcome: EvaluationOutcome::Inconclusive,
            execution_outcome: EvaluationOutcome::NotRun,
            diagnostics: Vec::new(),
        };
        repository.save(&artifact).unwrap();
        assert_eq!(repository.get("run-1").unwrap(), artifact);
        assert_eq!(repository.list().unwrap(), vec![artifact.clone()]);
        artifact.outcome = EvaluationOutcome::Passed;
        assert!(repository
            .save(&artifact)
            .unwrap_err()
            .contains("immutable"));
        assert_eq!(
            repository.get("run-1").unwrap().outcome,
            EvaluationOutcome::Inconclusive
        );
        fs::remove_dir_all(directory).unwrap();
    }
}
