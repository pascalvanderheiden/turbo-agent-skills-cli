use crate::{
    gh::CandidateFile,
    service::{DiscoveredSkill, SkillAdapter, StateRepository},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};
use tasc_core::{
    discovery::Diagnostic,
    security::{
        LifecycleSecurityEvidence, SecurityArtifact, SecurityCategory, SecurityCheckStatus,
        SecurityEvidence, SecurityFinding, SecurityPolicy, SecurityRequirementEvidence,
        SecurityRequirementStatus, SecuritySeverity, SecuritySkillSnapshot, SecuritySuppression,
        SecurityUnavailableCheck, SecurityUnsuppression,
    },
    Installation, LocalState, SkillIdentity, SourceRevision, LOCAL_STATE_VERSION,
};

static SECURITY_SEQUENCE: AtomicU64 = AtomicU64::new(0);
const RULE_VERSION: &str = "1";
const MAX_SKILL_BYTES: u64 = 50 * 1024 * 1024;
const MAX_SKILL_FILES: usize = 5_000;

pub trait SecurityArtifactRepository {
    fn save(&self, artifact: &SecurityArtifact) -> Result<(), String>;
    fn list(&self) -> Result<Vec<SecurityArtifact>, String>;
    fn get(&self, id: &str) -> Result<SecurityArtifact, String>;
}

pub trait SecuritySuppressionRepository {
    fn save_suppression(&self, suppression: &SecuritySuppression) -> Result<(), String>;
    fn list_suppressions(&self) -> Result<Vec<SecuritySuppression>, String>;
    fn save_unsuppression(&self, unsuppression: &SecurityUnsuppression) -> Result<(), String>;
    fn list_unsuppressions(&self) -> Result<Vec<SecurityUnsuppression>, String>;
}

#[derive(Debug, Clone)]
pub struct FsSecuritySuppressionRepository {
    directory: PathBuf,
}

impl FsSecuritySuppressionRepository {
    pub fn new(directory: PathBuf) -> Self {
        Self { directory }
    }
}

impl SecuritySuppressionRepository for FsSecuritySuppressionRepository {
    fn save_suppression(&self, suppression: &SecuritySuppression) -> Result<(), String> {
        save_immutable(
            &self.directory.join("suppressions"),
            &suppression.id,
            suppression,
            "Security Suppression",
        )
    }

    fn list_suppressions(&self) -> Result<Vec<SecuritySuppression>, String> {
        list_immutable(&self.directory.join("suppressions"), "Security Suppression")
    }

    fn save_unsuppression(&self, unsuppression: &SecurityUnsuppression) -> Result<(), String> {
        save_immutable(
            &self.directory.join("unsuppressions"),
            &unsuppression.id,
            unsuppression,
            "Security Unsuppression",
        )
    }

    fn list_unsuppressions(&self) -> Result<Vec<SecurityUnsuppression>, String> {
        list_immutable(
            &self.directory.join("unsuppressions"),
            "Security Unsuppression",
        )
    }
}

fn save_immutable<T: Serialize>(
    directory: &Path,
    id: &str,
    value: &T,
    kind: &str,
) -> Result<(), String> {
    fs::create_dir_all(directory)
        .map_err(|error| format!("cannot create {kind} directory: {error}"))?;
    let path = artifact_path(directory, id)?;
    let contents = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("cannot serialize {kind}: {error}"))?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                format!("{kind} records are immutable; this ID already exists")
            } else {
                format!("cannot create {kind}: {error}")
            }
        })?;
    if let Err(error) = file.write_all(&contents).and_then(|_| file.sync_all()) {
        drop(file);
        let _ = fs::remove_file(path);
        return Err(format!("cannot write {kind}: {error}"));
    }
    Ok(())
}

fn list_immutable<T: for<'de> Deserialize<'de>>(
    directory: &Path,
    kind: &str,
) -> Result<Vec<T>, String> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("cannot list {kind}s: {error}")),
    };
    let mut values = Vec::new();
    for entry in entries {
        let path = entry
            .map_err(|error| format!("cannot read {kind} entry: {error}"))?
            .path();
        if path.extension().is_none_or(|extension| extension != "json") {
            continue;
        }
        let metadata = fs::symlink_metadata(&path)
            .map_err(|error| format!("cannot inspect {kind}: {error}"))?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(format!("{kind}s must be regular, non-symlink files"));
        }
        let bytes = fs::read(&path).map_err(|error| format!("cannot read {kind}: {error}"))?;
        let record: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|error| format!("invalid {kind}: {error}"))?;
        let id = record
            .get("id")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| format!("{kind} is missing its ID"))?;
        if artifact_path(directory, id)? != path {
            return Err(format!("{kind} ID does not match its filename"));
        }
        let value =
            serde_json::from_value(record).map_err(|error| format!("invalid {kind}: {error}"))?;
        values.push(value);
    }
    Ok(values)
}

#[derive(Debug, Clone)]
pub struct FsSecurityArtifactRepository {
    directory: PathBuf,
}

impl FsSecurityArtifactRepository {
    pub fn new(directory: PathBuf) -> Self {
        Self { directory }
    }
}

impl SecurityArtifactRepository for FsSecurityArtifactRepository {
    fn save(&self, artifact: &SecurityArtifact) -> Result<(), String> {
        fs::create_dir_all(&self.directory)
            .map_err(|error| format!("cannot create Security Check directory: {error}"))?;
        let path = artifact_path(&self.directory, &artifact.id)?;
        let contents = serde_json::to_vec_pretty(artifact)
            .map_err(|error| format!("cannot serialize Security Check Artifact: {error}"))?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::AlreadyExists {
                    "Security Check Artifacts are immutable; this artifact ID already exists"
                        .to_string()
                } else {
                    format!("cannot create Security Check Artifact: {error}")
                }
            })?;
        if let Err(error) = file.write_all(&contents).and_then(|_| file.sync_all()) {
            drop(file);
            let _ = fs::remove_file(path);
            return Err(format!("cannot write Security Check Artifact: {error}"));
        }
        Ok(())
    }

    fn list(&self) -> Result<Vec<SecurityArtifact>, String> {
        let entries = match fs::read_dir(&self.directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(format!("cannot list Security Check Artifacts: {error}")),
        };
        let mut artifacts = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|error| format!("cannot read artifact entry: {error}"))?;
            let path = entry.path();
            if path.extension().is_none_or(|extension| extension != "json") {
                continue;
            }
            let metadata = fs::symlink_metadata(&path)
                .map_err(|error| format!("cannot inspect Security Check Artifact: {error}"))?;
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(
                    "Security Check Artifacts must be regular, non-symlink files".to_string(),
                );
            }
            let artifact: SecurityArtifact = serde_json::from_slice(
                &fs::read(&path)
                    .map_err(|error| format!("cannot read Security Check Artifact: {error}"))?,
            )
            .map_err(|error| format!("invalid Security Check Artifact: {error}"))?;
            if artifact_path(&self.directory, &artifact.id)? != path {
                return Err("Security Check Artifact ID does not match its filename".to_string());
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

    fn get(&self, id: &str) -> Result<SecurityArtifact, String> {
        let path = artifact_path(&self.directory, id)?;
        let metadata = fs::symlink_metadata(&path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                format!("no Security Check Artifact has ID '{id}'")
            } else {
                format!("cannot inspect Security Check Artifact: {error}")
            }
        })?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err("Security Check Artifacts must be regular, non-symlink files".to_string());
        }
        let artifact: SecurityArtifact = serde_json::from_slice(
            &fs::read(&path)
                .map_err(|error| format!("cannot read Security Check Artifact: {error}"))?,
        )
        .map_err(|error| format!("invalid Security Check Artifact: {error}"))?;
        if artifact_path(&self.directory, &artifact.id)? != path {
            return Err("Security Check Artifact ID does not match its filename".to_string());
        }
        Ok(artifact)
    }
}

pub struct SecurityService<D, R, A, S> {
    adapter: D,
    state_repository: R,
    artifact_repository: A,
    suppression_repository: S,
}

impl<
        D: SkillAdapter,
        R: StateRepository,
        A: SecurityArtifactRepository,
        S: SecuritySuppressionRepository,
    > SecurityService<D, R, A, S>
{
    pub fn new(
        adapter: D,
        state_repository: R,
        artifact_repository: A,
        suppression_repository: S,
    ) -> Self {
        Self {
            adapter,
            state_repository,
            artifact_repository,
            suppression_repository,
        }
    }

    pub fn check(
        &self,
        selectors: &[String],
        all_installed: bool,
        fail_on: Option<SecuritySeverity>,
    ) -> Result<SecurityArtifact, Diagnostic> {
        if (all_installed && !selectors.is_empty()) || (!all_installed && selectors.is_empty()) {
            return Err(selection_error(
                "Provide explicit skill selectors or use --all-installed, but not both.",
            ));
        }
        let state = self.state_repository.load().map_err(state_error)?;
        if state.version != LOCAL_STATE_VERSION {
            return Err(state_error(format!(
                "unsupported Local State version {}; expected {}",
                state.version, LOCAL_STATE_VERSION
            )));
        }
        let (selected, mut diagnostics, mut complete) = if all_installed {
            match self.adapter.discover() {
                Ok(discovered) => (
                    discovered.into_iter().map(selected_discovered).collect(),
                    Vec::new(),
                    true,
                ),
                Err(error) => (
                    Vec::new(),
                    vec![format!("local skill discovery was incomplete: {error}")],
                    false,
                ),
            }
        } else {
            let selected = match resolve_selection(&state, selectors, None) {
                Ok(selected) => selected,
                Err(_) => {
                    let discovered = self.adapter.discover().map_err(state_error)?;
                    resolve_selection(&state, selectors, Some(&discovered))?
                }
            };
            (selected, Vec::new(), true)
        };
        if selected.is_empty() {
            if complete {
                return Err(selection_error(
                    "No local skills are available for this Security Check.",
                ));
            }
        }

        let created_at_unix_ms = now_unix_ms();
        let sequence = SECURITY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let mut skills = Vec::new();
        let mut findings = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        for skill in selected {
            let key = (skill.identity.id.clone(), skill.path.clone());
            if !seen.insert(key) {
                return Err(selection_error(
                    "Selectors resolve to the same local skill more than once.",
                ));
            }
            let result = inspect_skill(&skill);
            complete &= result.complete;
            diagnostics.extend(result.diagnostics);
            findings.extend(result.findings);
            skills.push(result.snapshot);
        }
        skills.sort_by(|left, right| {
            left.identity
                .id
                .cmp(&right.identity.id)
                .then_with(|| left.path.cmp(&right.path))
        });
        findings.sort_by(|left, right| left.id.cmp(&right.id));
        let finding_fingerprints = findings
            .iter()
            .map(|finding| finding.fingerprint.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        let suppressed_fingerprints =
            active_suppressions(&self.suppression_repository, now_unix_ms())?
                .into_iter()
                .filter(|suppression| {
                    finding_fingerprints.contains(suppression.fingerprint.as_str())
                })
                .map(|suppression| suppression.fingerprint)
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect();
        let artifact = SecurityArtifact {
            schema_version: 1,
            id: format!(
                "security-{}-{}-{sequence}",
                created_at_unix_ms,
                std::process::id()
            ),
            created_at_unix_ms,
            completed_at_unix_ms: now_unix_ms(),
            selectors: selectors.to_vec(),
            all_installed,
            analyzer: "tasc-static-security".to_string(),
            analyzer_version: env!("CARGO_PKG_VERSION").to_string(),
            rule_version: RULE_VERSION.to_string(),
            status: if complete {
                SecurityCheckStatus::Complete
            } else {
                SecurityCheckStatus::Partial
            },
            policy: SecurityPolicy {
                fail_on,
                minimum_confidence: 90,
            },
            skills,
            findings,
            suppressed_fingerprints,
            unavailable_checks: vec![
                SecurityUnavailableCheck {
                    name: "external_reference_reputation".to_string(),
                    status: "not_run".to_string(),
                    reason: "Network-dependent reputation checks are not fetched implicitly."
                        .to_string(),
                },
                SecurityUnavailableCheck {
                    name: "dynamic_execution".to_string(),
                    status: "not_run".to_string(),
                    reason: "Skill content is analyzed statically and is never executed."
                        .to_string(),
                },
            ],
            diagnostics,
        };
        self.artifact_repository
            .save(&artifact)
            .map_err(state_error)?;
        Ok(artifact)
    }

    pub fn suppress(
        &self,
        fingerprint: &str,
        reason: &str,
        expires_in_days: u64,
    ) -> Result<SecuritySuppression, Diagnostic> {
        if !is_fingerprint(fingerprint) {
            return Err(selection_error(
                "A suppression must target one exact Security Finding fingerprint.",
            ));
        }
        if reason.trim().is_empty() || !(1..=365).contains(&expires_in_days) {
            return Err(selection_error(
                "Provide a suppression reason and an expiry between 1 and 365 days.",
            ));
        }
        let artifacts = self.artifact_repository.list().map_err(state_error)?;
        if !artifacts.iter().any(|artifact| {
            artifact
                .findings
                .iter()
                .any(|finding| finding.fingerprint == fingerprint)
        }) {
            return Err(selection_error(
                "No recorded Security Finding has this fingerprint.",
            ));
        }
        if active_suppressions(&self.suppression_repository, now_unix_ms())?
            .iter()
            .any(|suppression| suppression.fingerprint == fingerprint)
        {
            return Err(selection_error(
                "This Security Finding already has an active suppression.",
            ));
        }
        let created_at_unix_ms = now_unix_ms();
        let sequence = SECURITY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let suppression = SecuritySuppression {
            id: format!(
                "suppression-{created_at_unix_ms}-{}-{sequence}",
                std::process::id()
            ),
            fingerprint: fingerprint.to_string(),
            reason: reason.trim().to_string(),
            created_at_unix_ms,
            expires_at_unix_ms: created_at_unix_ms
                .saturating_add(u128::from(expires_in_days).saturating_mul(86_400_000)),
        };
        self.suppression_repository
            .save_suppression(&suppression)
            .map_err(state_error)?;
        Ok(suppression)
    }

    pub fn unsuppress(
        &self,
        fingerprint: &str,
        reason: &str,
    ) -> Result<SecurityUnsuppression, Diagnostic> {
        if !is_fingerprint(fingerprint) || reason.trim().is_empty() {
            return Err(selection_error(
                "Provide an exact finding fingerprint and an unsuppression reason.",
            ));
        }
        let now = now_unix_ms();
        let suppression = active_suppressions(&self.suppression_repository, now)?
            .into_iter()
            .find(|suppression| suppression.fingerprint == fingerprint)
            .ok_or_else(|| selection_error("This Security Finding has no active suppression."))?;
        let sequence = SECURITY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let unsuppression = SecurityUnsuppression {
            id: format!("unsuppression-{now}-{}-{sequence}", std::process::id()),
            suppression_id: suppression.id,
            reason: reason.trim().to_string(),
            created_at_unix_ms: now,
        };
        self.suppression_repository
            .save_unsuppression(&unsuppression)
            .map_err(state_error)?;
        Ok(unsuppression)
    }

    pub fn list(&self) -> Result<Vec<SecurityArtifact>, Diagnostic> {
        self.artifact_repository.list().map_err(state_error)
    }

    pub fn show(&self, id: &str) -> Result<SecurityArtifact, Diagnostic> {
        self.artifact_repository.get(id).map_err(state_error)
    }
}

#[derive(Clone)]
struct SelectedSkill {
    identity: SkillIdentity,
    path: String,
    source_revision: Option<String>,
}

fn resolve_selection(
    state: &LocalState,
    selectors: &[String],
    discovered: Option<&[DiscoveredSkill]>,
) -> Result<Vec<SelectedSkill>, Diagnostic> {
    let mut selected = Vec::new();
    let mut seen_selectors = std::collections::BTreeSet::new();
    for selector in selectors {
        if !seen_selectors.insert(selector) {
            return Err(selection_error(&format!(
                "Skill selector '{selector}' was provided more than once."
            )));
        }
        if let Some(installation) = state.installations.iter().find(|installation| {
            installation.id == *selector
                || state
                    .identities
                    .iter()
                    .find(|identity| identity.id == installation.skill_id)
                    .and_then(|identity| {
                        state
                            .targets
                            .iter()
                            .find(|target| target.id == installation.target_id)
                            .map(|target| {
                                format!("installation:{}@{}", identity.id, target.id) == *selector
                            })
                    })
                    .unwrap_or(false)
        }) {
            selected.push(selected_installation(state, installation)?);
            continue;
        }
        let identities = state
            .identities
            .iter()
            .filter(|identity| identity.id == *selector || identity.name == *selector)
            .collect::<Vec<_>>();
        if identities.len() > 1 {
            return Err(selection_error(&format!(
                "Selector '{selector}' matches multiple Skill Identities; use an Installation selector."
            )));
        }
        if let Some(identity) = identities.first() {
            let installations = state
                .installations
                .iter()
                .filter(|installation| installation.skill_id == identity.id)
                .collect::<Vec<_>>();
            return match installations.as_slice() {
                [installation] => {
                    selected.push(selected_installation(state, installation)?);
                    continue;
                }
                [] => {
                    if let Some(skill) = discovered
                        .into_iter()
                        .flatten()
                        .find(|skill| skill.identity.id == identity.id)
                    {
                        selected.push(selected_discovered(skill.clone()));
                        continue;
                    }
                    return Err(selection_error(&format!(
                        "Skill Identity '{}' has no local Installation; select its Skill Workspace path.",
                        identity.id
                    )));
                }
                _ => Err(selection_error(&format!(
                    "Skill Identity '{}' has multiple Installations; select one Installation explicitly.",
                    identity.id
                ))),
            };
        }
        if let Some(matches) = discovered.map(|skills| {
            skills
                .iter()
                .filter(|skill| {
                    skill.identity.id == *selector
                        || skill.identity.name == *selector
                        || format!("installation:{}@{}", skill.identity.id, skill.target.id)
                            == *selector
                })
                .collect::<Vec<_>>()
        }) {
            match matches.as_slice() {
                [skill] => {
                    selected.push(selected_discovered((*skill).clone()));
                    continue;
                }
                many if !many.is_empty() => {
                    return Err(selection_error(&format!(
                        "Selector '{selector}' matches multiple local Installations; select one Installation explicitly."
                    )));
                }
                _ => {}
            }
        }
        selected.push(selected_workspace(selector)?);
    }
    Ok(selected)
}

fn selected_installation(
    state: &LocalState,
    installation: &Installation,
) -> Result<SelectedSkill, Diagnostic> {
    let identity = state
        .identities
        .iter()
        .find(|identity| identity.id == installation.skill_id)
        .ok_or_else(|| selection_error("Installation refers to an unknown Skill Identity."))?;
    Ok(SelectedSkill {
        identity: identity.clone(),
        path: installation.path.clone(),
        source_revision: installation.source_revision.clone(),
    })
}

fn selected_workspace(selector: &str) -> Result<SelectedSkill, Diagnostic> {
    let path = Path::new(selector);
    let metadata = fs::symlink_metadata(path).map_err(|_| {
        selection_error(&format!(
            "No Skill Identity, Installation, or local Skill Workspace matches '{selector}'."
        ))
    })?;
    if metadata.file_type().is_symlink() {
        return Err(selection_error(
            "A selected Skill Workspace must not be a symbolic link.",
        ));
    }
    let path = if metadata.is_file() && path.file_name().is_some_and(|name| name == "SKILL.md") {
        path.parent()
            .ok_or_else(|| selection_error("The selected SKILL.md has no parent directory."))?
    } else if metadata.is_dir() {
        path
    } else {
        return Err(selection_error(
            "A selected Skill Workspace must be a directory containing SKILL.md.",
        ));
    };
    let path = fs::canonicalize(path).map_err(|error| state_error(error.to_string()))?;
    if !path.join("SKILL.md").is_file() {
        return Err(selection_error(
            "The selected Skill Workspace does not contain SKILL.md.",
        ));
    }
    let display = path.to_string_lossy().into_owned();
    let mut hasher = Sha256::new();
    hasher.update(display.as_bytes());
    let workspace_id = hex_digest(hasher.finalize().as_slice());
    let name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    Ok(SelectedSkill {
        identity: SkillIdentity {
            id: format!("workspace:{workspace_id}"),
            name,
            source: format!("file://{display}"),
            relative_path: ".".to_string(),
        },
        path: display,
        source_revision: None,
    })
}

fn selected_discovered(skill: DiscoveredSkill) -> SelectedSkill {
    SelectedSkill {
        identity: skill.identity,
        path: skill.path,
        source_revision: skill.source_revision,
    }
}

struct Inspection {
    snapshot: SecuritySkillSnapshot,
    findings: Vec<SecurityFinding>,
    diagnostics: Vec<String>,
    complete: bool,
}

struct ContentFile {
    relative_path: String,
    bytes: Vec<u8>,
    executable: bool,
}

pub fn inspect_source_candidate(
    identity: &SkillIdentity,
    revision: SourceRevision,
    files: Vec<CandidateFile>,
    unavailable_reason: Option<String>,
) -> LifecycleSecurityEvidence {
    let mut diagnostics = Vec::new();
    let mut complete = unavailable_reason.is_none();
    if let Some(reason) = &unavailable_reason {
        diagnostics.push(reason.clone());
    }
    let mut total = 0_u64;
    let mut content_files = Vec::new();
    if files.len() > MAX_SKILL_FILES {
        diagnostics.push(format!(
            "candidate contains more than the {MAX_SKILL_FILES}-file inspection limit"
        ));
        complete = false;
    }
    for file in files.into_iter().take(MAX_SKILL_FILES) {
        if file.path.is_empty()
            || Path::new(&file.path)
                .components()
                .any(|component| matches!(component, std::path::Component::ParentDir))
        {
            diagnostics.push("candidate contains an unsafe file path".to_string());
            complete = false;
            continue;
        }
        if total.saturating_add(file.bytes.len() as u64) > MAX_SKILL_BYTES {
            diagnostics.push(format!(
                "candidate exceeds the {} MiB inspection limit",
                MAX_SKILL_BYTES / (1024 * 1024)
            ));
            complete = false;
            break;
        }
        total += file.bytes.len() as u64;
        content_files.push(ContentFile {
            relative_path: file.path,
            bytes: file.bytes,
            executable: file.executable,
        });
    }
    content_files.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    if !content_files
        .iter()
        .any(|file| file.relative_path == "SKILL.md")
    {
        diagnostics.push("candidate does not contain its selected SKILL.md".to_string());
        complete = false;
    }
    let digest = content_digest(&content_files);
    let mut findings = analyze_content_files(
        identity,
        &revision,
        &digest,
        content_files,
        &mut diagnostics,
        &mut complete,
    );
    findings.sort_by(|left, right| left.id.cmp(&right.id));
    LifecycleSecurityEvidence {
        policy: SecurityPolicy {
            fail_on: Some(SecuritySeverity::High),
            minimum_confidence: 90,
        },
        source_revision: revision,
        content_digest: Some(digest),
        required_checks: vec![SecurityRequirementEvidence {
            name: "static_candidate_content".to_string(),
            status: if complete {
                SecurityRequirementStatus::Complete
            } else if unavailable_reason.is_some() {
                SecurityRequirementStatus::Unavailable
            } else {
                SecurityRequirementStatus::Inconclusive
            },
            reason: (!complete).then(|| {
                unavailable_reason.clone().unwrap_or_else(|| {
                    "Candidate content could not be inspected completely within policy bounds."
                        .to_string()
                })
            }),
        }],
        findings,
        diagnostics,
    }
}

fn analyze_content_files(
    identity: &SkillIdentity,
    source_revision: &SourceRevision,
    digest: &str,
    content_files: Vec<ContentFile>,
    diagnostics: &mut Vec<String>,
    complete: &mut bool,
) -> Vec<SecurityFinding> {
    let source_revision = Some(source_revision.clone());
    let mut findings = Vec::new();
    for file in content_files {
        if file.bytes.contains(&0) {
            diagnostics.push(format!(
                "skipped binary file '{}' during text analysis",
                file.relative_path
            ));
            *complete = false;
            continue;
        }
        let text = String::from_utf8_lossy(&file.bytes);
        let mut private_key_block = false;
        for (index, line) in text.lines().enumerate() {
            let line_number = index + 1;
            let lower = line.to_ascii_lowercase();
            if lower.contains("-----begin") && lower.contains("private key-----") {
                private_key_block = true;
            }
            let sanitized = redact_possible_secrets(line);
            for detection in detect_line(&lower, line) {
                let mut finding = make_finding(
                    &identity.id,
                    &file.relative_path,
                    line_number,
                    &detection,
                    &sanitized,
                    &source_revision,
                    digest,
                );
                if private_key_block {
                    finding.evidence[0].excerpt = "[REDACTED PRIVATE KEY MATERIAL]".to_string();
                    finding.category = SecurityCategory::ExposedSecret;
                    finding.detector = "private_key_material".to_string();
                    finding.rationale = "A private key material block is present.".to_string();
                }
                findings.push(finding);
            }
            if private_key_block && lower.contains("-----end") && lower.contains("private key-----")
            {
                private_key_block = false;
            }
        }
        if file.executable {
            let detection = Detection {
                category: SecurityCategory::RiskyExecutableContent,
                detector: "executable_file".to_string(),
                severity: SecuritySeverity::Medium,
                confidence: 92,
                rationale:
                    "This skill resource is marked executable and may run when invoked externally."
                        .to_string(),
                recommendation:
                    "Review the script in the selected Skill Workspace; TASC never executes it."
                        .to_string(),
            };
            findings.push(make_finding(
                &identity.id,
                &file.relative_path,
                1,
                &detection,
                "[executable file]",
                &source_revision,
                digest,
            ));
        }
    }
    findings
}

fn inspect_skill(skill: &SelectedSkill) -> Inspection {
    let mut diagnostics = Vec::new();
    let mut complete = true;
    let root = Path::new(&skill.path);
    let canonical_root = match fs::canonicalize(root) {
        Ok(path) => path,
        Err(error) => {
            diagnostics.push(format!("cannot resolve selected Skill Workspace: {error}"));
            return Inspection {
                snapshot: snapshot(skill, None),
                findings: Vec::new(),
                diagnostics,
                complete: false,
            };
        }
    };
    let metadata = match fs::symlink_metadata(root) {
        Ok(metadata) => metadata,
        Err(error) => {
            diagnostics.push(format!("cannot inspect selected Skill Workspace: {error}"));
            return Inspection {
                snapshot: snapshot(skill, None),
                findings: Vec::new(),
                diagnostics,
                complete: false,
            };
        }
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        diagnostics.push("selected Skill Workspace is not a regular directory".to_string());
        return Inspection {
            snapshot: snapshot(skill, None),
            findings: Vec::new(),
            diagnostics,
            complete: false,
        };
    }
    let mut files = Vec::new();
    if let Err(error) = collect_files(
        &canonical_root,
        &canonical_root,
        &mut files,
        &mut diagnostics,
    ) {
        diagnostics.push(error);
        complete = false;
    }
    complete &= diagnostics.is_empty();
    files.sort_by(|left, right| left.0.cmp(&right.0));
    if files.len() > MAX_SKILL_FILES {
        files.truncate(MAX_SKILL_FILES);
        diagnostics.push(format!(
            "analysis stopped after the {MAX_SKILL_FILES}-file inspection limit"
        ));
        complete = false;
    }
    let mut total = 0_u64;
    let mut content_files = Vec::new();
    for (relative_path, file_path) in files {
        let metadata = match fs::symlink_metadata(&file_path) {
            Ok(metadata) => metadata,
            Err(error) => {
                diagnostics.push(format!("cannot inspect '{}': {error}", relative_path));
                complete = false;
                continue;
            }
        };
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            diagnostics.push(format!("skipped non-regular file '{relative_path}'"));
            complete = false;
            continue;
        }
        let remaining = MAX_SKILL_BYTES.saturating_sub(total);
        if metadata.len() > remaining {
            diagnostics.push(format!(
                "analysis stopped at the {} MiB inspection limit",
                MAX_SKILL_BYTES / (1024 * 1024)
            ));
            complete = false;
            break;
        }
        match read_regular_file(&file_path, &metadata, remaining) {
            Ok(bytes) => {
                total = total.saturating_add(bytes.len() as u64);
                content_files.push(ContentFile {
                    relative_path,
                    bytes,
                    executable: file_is_executable(&metadata),
                })
            }
            Err(error) => {
                diagnostics.push(format!("cannot read '{relative_path}': {error}"));
                complete = false;
            }
        }
    }
    let digest = content_digest(&content_files);
    let source_revision = skill
        .source_revision
        .as_ref()
        .map(|revision| SourceRevision {
            kind: "observed".to_string(),
            value: revision.clone(),
        });
    let snapshot = SecuritySkillSnapshot {
        identity: skill.identity.clone(),
        source_revision: source_revision.clone(),
        content_digest: Some(digest.clone()),
        path: canonical_root.to_string_lossy().into_owned(),
    };
    let mut findings = Vec::new();
    for file in content_files {
        if file.bytes.contains(&0) {
            diagnostics.push(format!(
                "skipped binary file '{}' during text analysis",
                file.relative_path
            ));
            complete = false;
            continue;
        }
        let text = String::from_utf8_lossy(&file.bytes);
        let mut private_key_block = false;
        for (index, line) in text.lines().enumerate() {
            let line_number = index + 1;
            let lower = line.to_ascii_lowercase();
            if lower.contains("-----begin") && lower.contains("private key-----") {
                private_key_block = true;
            }
            let sanitized = redact_possible_secrets(line);
            for detection in detect_line(&lower, line) {
                let mut finding = make_finding(
                    &skill.identity.id,
                    &file.relative_path,
                    line_number,
                    &detection,
                    &sanitized,
                    &source_revision,
                    &digest,
                );
                if private_key_block {
                    finding.evidence[0].excerpt = "[REDACTED PRIVATE KEY MATERIAL]".to_string();
                    finding.category = SecurityCategory::ExposedSecret;
                    finding.detector = "private_key_material".to_string();
                    finding.rationale = "A private key material block is present.".to_string();
                }
                findings.push(finding);
            }
            if private_key_block && lower.contains("-----end") && lower.contains("private key-----")
            {
                private_key_block = false;
            }
        }
        if file.executable {
            let detection = Detection {
                category: SecurityCategory::RiskyExecutableContent,
                detector: "executable_file".to_string(),
                severity: SecuritySeverity::Medium,
                confidence: 92,
                rationale:
                    "This skill resource is marked executable and may run when invoked externally."
                        .to_string(),
                recommendation:
                    "Review the script in the selected Skill Workspace; TASC never executes it."
                        .to_string(),
            };
            findings.push(make_finding(
                &skill.identity.id,
                &file.relative_path,
                1,
                &detection,
                "[executable file]",
                &source_revision,
                &digest,
            ));
        }
    }
    Inspection {
        snapshot,
        findings,
        diagnostics,
        complete,
    }
}

fn collect_files(
    root: &Path,
    directory: &Path,
    files: &mut Vec<(String, PathBuf)>,
    diagnostics: &mut Vec<String>,
) -> Result<(), String> {
    let entries = fs::read_dir(directory).map_err(|error| {
        format!(
            "cannot inspect skill directory '{}': {error}",
            directory.display()
        )
    })?;
    for entry in entries {
        if files.len() >= MAX_SKILL_FILES {
            if !diagnostics
                .iter()
                .any(|message| message.contains("file inspection limit"))
            {
                diagnostics.push(format!(
                    "analysis stopped after the {MAX_SKILL_FILES}-file inspection limit"
                ));
            }
            return Ok(());
        }
        let entry = entry.map_err(|error| format!("cannot inspect skill entry: {error}"))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|error| format!("cannot inspect skill entry '{}': {error}", path.display()))?;
        let name = entry.file_name();
        if file_type.is_dir() {
            if name == ".git" {
                continue;
            }
            collect_files(root, &path, files, diagnostics)?;
        } else if file_type.is_symlink() {
            diagnostics.push(format!(
                "skipped symbolic link '{}'",
                path.strip_prefix(root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/")
            ));
        } else if file_type.is_file() {
            files.push((
                path.strip_prefix(root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/"),
                path,
            ));
        }
    }
    Ok(())
}

fn content_digest(files: &[ContentFile]) -> String {
    let mut hasher = Sha256::new();
    for file in files {
        hasher.update(file.relative_path.as_bytes());
        hasher.update([0]);
        hasher.update([u8::from(file.executable)]);
        hasher.update(&file.bytes);
        hasher.update([0]);
    }
    hex_digest(hasher.finalize().as_slice())
}

fn active_suppressions<S: SecuritySuppressionRepository>(
    repository: &S,
    now: u128,
) -> Result<Vec<SecuritySuppression>, Diagnostic> {
    let suppressions = repository.list_suppressions().map_err(state_error)?;
    let revoked = repository
        .list_unsuppressions()
        .map_err(state_error)?
        .into_iter()
        .map(|unsuppression| unsuppression.suppression_id)
        .collect::<std::collections::BTreeSet<_>>();
    Ok(suppressions
        .into_iter()
        .filter(|suppression| {
            suppression.expires_at_unix_ms > now && !revoked.contains(&suppression.id)
        })
        .collect())
}

fn is_fingerprint(fingerprint: &str) -> bool {
    fingerprint.len() == 64
        && fingerprint
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn snapshot(skill: &SelectedSkill, content_digest: Option<String>) -> SecuritySkillSnapshot {
    SecuritySkillSnapshot {
        identity: skill.identity.clone(),
        source_revision: skill
            .source_revision
            .as_ref()
            .map(|revision| SourceRevision {
                kind: "observed".to_string(),
                value: revision.clone(),
            }),
        content_digest,
        path: skill.path.clone(),
    }
}

struct Detection {
    category: SecurityCategory,
    detector: String,
    severity: SecuritySeverity,
    confidence: u8,
    rationale: String,
    recommendation: String,
}

fn detect_line(lower: &str, original: &str) -> Vec<Detection> {
    let mut detections = Vec::new();
    let secret = lower.contains("-----begin") && lower.contains("private key-----")
        || contains_token_prefix(original, "ghp_", 20)
        || contains_token_prefix(original, "gho_", 20)
        || contains_token_prefix(original, "ghu_", 20)
        || contains_token_prefix(original, "ghs_", 20)
        || contains_token_prefix(original, "github_pat_", 20)
        || contains_token_prefix(original, "AKIA", 16)
        || lower.contains("authorization: bearer ")
        || has_secret_assignment(lower);
    if secret {
        detections.push(Detection {
            category: SecurityCategory::ExposedSecret,
            detector: "credential_pattern".to_string(),
            severity: SecuritySeverity::Critical,
            confidence: if lower.contains("-----begin")
                || lower.contains("authorization: bearer")
                || contains_token_prefix(original, "ghp_", 20)
                || contains_token_prefix(original, "AKIA", 16)
            {
                99
            } else {
                92
            },
            rationale: "Content resembles a credential or private secret.".to_string(),
            recommendation:
                "Revoke the exposed credential and remove it from the selected Skill Workspace."
                    .to_string(),
        });
    }
    if [
        "ignore all previous instructions",
        "ignore previous instructions",
        "disregard previous instructions",
        "override system prompt",
    ]
    .iter()
    .any(|pattern| lower.contains(pattern))
    {
        detections.push(Detection {
            category: SecurityCategory::InjectionOrExfiltration,
            detector: "instruction_override".to_string(),
            severity: SecuritySeverity::High,
            confidence: 98,
            rationale: "The instruction attempts to override higher-priority instructions."
                .to_string(),
            recommendation: "Review the instruction and remove untrusted prompt-override content."
                .to_string(),
        });
    }
    if (lower.contains("send") || lower.contains("upload") || lower.contains("exfiltrat"))
        && [
            "credentials",
            "secrets",
            "environment variables",
            "private key",
            "tokens",
        ]
        .iter()
        .any(|term| lower.contains(term))
    {
        detections.push(Detection {
            category: SecurityCategory::InjectionOrExfiltration,
            detector: "sensitive_data_transfer".to_string(),
            severity: SecuritySeverity::Critical,
            confidence: 96,
            rationale: "The instruction describes transferring credentials or other sensitive data."
                .to_string(),
            recommendation: "Remove the transfer instruction and verify that no data was disclosed."
                .to_string(),
        });
    }
    if [
        "disable safety",
        "bypass approval",
        "without user confirmation",
        "do not ask for permission",
        "permissions: all",
        "allow: [\"*\"]",
    ]
    .iter()
    .any(|pattern| lower.contains(pattern))
    {
        detections.push(Detection {
            category: SecurityCategory::UnsafeInstruction,
            detector: "unsafe_permission_or_instruction".to_string(),
            severity: SecuritySeverity::High,
            confidence: 94,
            rationale:
                "The content appears to disable safeguards or grant unrestricted permissions."
                    .to_string(),
            recommendation:
                "Narrow the permission or require confirmation for consequential actions."
                    .to_string(),
        });
    }
    if lower.contains("curl") && (lower.contains("| sh") || lower.contains("| bash"))
        || lower.contains("wget") && (lower.contains("| sh") || lower.contains("| bash"))
    {
        detections.push(Detection {
            category: SecurityCategory::RiskyExecutableContent,
            detector: "download_pipe_to_shell".to_string(),
            severity: SecuritySeverity::High,
            confidence: 98,
            rationale: "The command downloads content and pipes it directly to a shell.".to_string(),
            recommendation: "Pin and verify downloaded content before any separate, explicitly approved execution."
                .to_string(),
        });
        detections.push(Detection {
            category: SecurityCategory::SupplyChainIndicator,
            detector: "unverified_remote_installer".to_string(),
            severity: SecuritySeverity::High,
            confidence: 98,
            rationale: "A remote installer is consumed without local integrity verification."
                .to_string(),
            recommendation: "Use a verified, revision-pinned dependency source.".to_string(),
        });
    }
    if lower.contains("rm -rf /")
        || lower.contains("chmod 777")
        || lower.contains("--no-verify")
        || lower.contains("sudo ")
    {
        detections.push(Detection {
            category: SecurityCategory::RiskyExecutableContent,
            detector: "high_impact_shell_command".to_string(),
            severity: SecuritySeverity::High,
            confidence: 95,
            rationale: "The content contains a command with broad system impact.".to_string(),
            recommendation: "Remove or narrowly constrain the command and require explicit review."
                .to_string(),
        });
    }
    if lower.contains("javascript:")
        || lower.contains("data:text/html")
        || lower.contains("http://127.0.0.1")
        || lower.contains("http://localhost")
        || lower.contains("http://169.254.")
        || lower.contains("xn--")
        || url_contains_credentials(lower)
    {
        detections.push(Detection {
            category: SecurityCategory::SuspiciousReference,
            detector: "suspicious_reference".to_string(),
            severity: SecuritySeverity::High,
            confidence: 93,
            rationale: "The reference uses a deceptive, local, or credential-bearing URL form."
                .to_string(),
            recommendation:
                "Verify the reference manually; no external reference is fetched by this check."
                    .to_string(),
        });
    }
    if (lower.contains("pip install ") || lower.contains("npm install "))
        && !lower.contains("==")
        && !lower.contains("@")
        && !lower.contains("package-lock")
    {
        detections.push(Detection {
            category: SecurityCategory::SupplyChainIndicator,
            detector: "unpinned_dependency_install".to_string(),
            severity: SecuritySeverity::Medium,
            confidence: 91,
            rationale: "A package installation appears not to pin an exact dependency version."
                .to_string(),
            recommendation: "Pin dependencies and verify their provenance before installation."
                .to_string(),
        });
    }
    detections
}

fn make_finding(
    skill_id: &str,
    path: &str,
    line: usize,
    detection: &Detection,
    excerpt: &str,
    source_revision: &Option<SourceRevision>,
    content_digest: &str,
) -> SecurityFinding {
    let material = format!(
        "{skill_id}\0{}\0{}\0{line}\0{content_digest}",
        detection.detector, path
    );
    let mut hasher = Sha256::new();
    hasher.update(material.as_bytes());
    let fingerprint = hex_digest(hasher.finalize().as_slice());
    SecurityFinding {
        id: format!("finding:{fingerprint}"),
        fingerprint,
        category: detection.category,
        detector: detection.detector.clone(),
        severity: detection.severity,
        confidence_score: detection.confidence,
        deterministic: true,
        rationale: detection.rationale.clone(),
        recommendation: detection.recommendation.clone(),
        skill_id: skill_id.to_string(),
        evidence: vec![SecurityEvidence {
            path: path.to_string(),
            line,
            excerpt: excerpt.to_string(),
            source_revision: source_revision.clone(),
            content_digest: content_digest.to_string(),
        }],
    }
}

const SECRET_ASSIGNMENT_KEYS: [&str; 13] = [
    "secret_access_key",
    "api_key",
    "apikey",
    "access_key",
    "client_secret",
    "secret_key",
    "private_key",
    "privatekey",
    "private key",
    "private-key",
    "password",
    "token",
    "secret",
];

fn secret_assignment_values(lower: &str) -> Vec<&str> {
    SECRET_ASSIGNMENT_KEYS
        .iter()
        .flat_map(|key| {
            lower.match_indices(key).filter_map(move |(index, _)| {
                let suffix = lower[index + key.len()..]
                    .trim_start_matches(['\'', '"', '`'])
                    .trim_start();
                suffix
                    .strip_prefix('=')
                    .or_else(|| suffix.strip_prefix(':'))
                    .map(str::trim_start)
            })
        })
        .collect()
}

fn has_secret_assignment(lower: &str) -> bool {
    secret_assignment_values(lower).into_iter().any(|suffix| {
        let value = suffix
            .trim_start()
            .trim_start_matches(['\'', '"', '`'])
            .split([' ', '\t', '\'', '"', '`', ',', ';'])
            .next()
            .unwrap_or_default();
        value.chars().count() >= 8
            && ![
                "example",
                "placeholder",
                "your_",
                "changeme",
                "xxxx",
                "${",
                "{{",
                "<",
            ]
            .iter()
            .any(|placeholder| value.contains(placeholder))
    })
}

pub(crate) fn has_multiline_secret_declaration(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    secret_assignment_values(&lower).into_iter().any(|suffix| {
        let value = suffix
            .trim_start()
            .trim_start_matches(['\'', '"', '`'])
            .trim_start();
        if value.is_empty() || value.starts_with('#') {
            return true;
        }
        let Some(marker) = value.chars().next() else {
            return true;
        };
        if !matches!(marker, '|' | '>') {
            return false;
        }
        let rest = value[marker.len_utf8()..].trim_start();
        let header = rest.split_whitespace().next().unwrap_or_default();
        header.is_empty()
            || header.starts_with('#')
            || header
                .chars()
                .all(|character| character.is_ascii_digit() || matches!(character, '-' | '+'))
    })
}

fn contains_token_prefix(text: &str, prefix: &str, minimum_length: usize) -> bool {
    text.match_indices(prefix).any(|(index, _)| {
        text[index + prefix.len()..]
            .chars()
            .take_while(|character| character.is_ascii_alphanumeric() || *character == '_')
            .count()
            >= minimum_length
    })
}

pub(crate) fn redact_possible_secrets(line: &str) -> String {
    let lower = line.to_ascii_lowercase();
    if lower.contains("-----begin") && lower.contains("private key-----") {
        return "[REDACTED PRIVATE KEY MATERIAL]".to_string();
    }
    let mut output = line.to_string();
    for prefix in ["ghp_", "gho_", "ghu_", "ghs_", "github_pat_", "AKIA"] {
        let mut cursor = 0;
        while let Some(relative) = output[cursor..].find(prefix) {
            let start = cursor + relative;
            let end = output[start + prefix.len()..]
                .char_indices()
                .find(|(_, character)| !character.is_ascii_alphanumeric() && *character != '_')
                .map(|(offset, _)| start + prefix.len() + offset)
                .unwrap_or(output.len());
            output.replace_range(start..end, "[REDACTED]");
            cursor = start + "[REDACTED]".len();
        }
    }
    if has_secret_assignment(&output.to_ascii_lowercase()) {
        if let Some(separator) = output.find('=').or_else(|| output.find(':')) {
            output.truncate(separator + 1);
            output.push_str(" [REDACTED]");
        }
    }
    if let Some(index) = output.to_ascii_lowercase().find("authorization: bearer ") {
        output.truncate(index + "authorization: bearer ".len());
        output.push_str("[REDACTED]");
    }
    let mut cursor = 0;
    while let Some(relative) = output[cursor..].find("://") {
        let authority_start = cursor + relative + 3;
        let authority_end = output[authority_start..]
            .find(['/', '?', '#', ' ', '\t'])
            .map(|offset| authority_start + offset)
            .unwrap_or(output.len());
        if let Some(at) = output[authority_start..authority_end].find('@') {
            let at = authority_start + at;
            output.replace_range(authority_start..at, "[REDACTED]");
            cursor = authority_start + "[REDACTED]".len() + 1;
        } else {
            cursor = authority_end;
        }
        if cursor >= output.len() {
            break;
        }
    }
    output.chars().take(500).collect()
}

fn url_contains_credentials(line: &str) -> bool {
    line.split_whitespace().any(|part| {
        part.find("://")
            .and_then(|scheme| part[scheme + 3..].find('@'))
            .is_some()
    })
}

fn read_regular_file(
    path: &Path,
    expected_metadata: &fs::Metadata,
    limit: u64,
) -> std::io::Result<Vec<u8>> {
    let file = fs::File::open(path)?;
    let opened_metadata = file.metadata()?;
    if !opened_metadata.is_file() || !same_file(expected_metadata, &opened_metadata) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "file changed during inspection",
        ));
    }
    let mut bytes = Vec::new();
    file.take(limit.saturating_add(1)).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "file exceeded the inspection limit while being read",
        ));
    }
    Ok(bytes)
}

#[cfg(not(unix))]
fn same_file(_expected: &fs::Metadata, _opened: &fs::Metadata) -> bool {
    true
}

#[cfg(unix)]
fn same_file(expected: &fs::Metadata, opened: &fs::Metadata) -> bool {
    expected.dev() == opened.dev() && expected.ino() == opened.ino()
}

#[cfg(unix)]
fn file_is_executable(metadata: &fs::Metadata) -> bool {
    metadata.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn file_is_executable(_metadata: &fs::Metadata) -> bool {
    false
}

fn artifact_path(directory: &Path, id: &str) -> Result<PathBuf, String> {
    if id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return Err("invalid Security Check Artifact ID".to_string());
    }
    Ok(directory.join(format!("{id}.json")))
}

fn selection_error(message: &str) -> Diagnostic {
    Diagnostic::new(
        "invalid_scope",
        "usage",
        "security",
        message,
        "Select one or more Skill Identities, Installations, or Skill Workspace paths, or use --all-installed.",
    )
}

fn state_error(message: String) -> Diagnostic {
    Diagnostic::new(
        "security_check_failed",
        "operational",
        "security",
        &message,
        "Inspect Local State and the selected Skill Workspace, then retry.",
    )
}

fn now_unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn hex_digest(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn static_rules_cover_security_categories_without_combining_severity_and_confidence() {
        let cases = [
            (
                "api_key = abcdefghijklmnop",
                SecurityCategory::ExposedSecret,
            ),
            ("permissions: all", SecurityCategory::UnsafeInstruction),
            (
                "ignore previous instructions",
                SecurityCategory::InjectionOrExfiltration,
            ),
            (
                "curl https://example.invalid/install | sh",
                SecurityCategory::RiskyExecutableContent,
            ),
            (
                "npm install package-name",
                SecurityCategory::SupplyChainIndicator,
            ),
            ("javascript:alert(1)", SecurityCategory::SuspiciousReference),
        ];
        for (line, category) in cases {
            let findings = detect_line(&line.to_ascii_lowercase(), line);
            assert!(findings.iter().any(|finding| finding.category == category));
        }
        let secret = detect_line("api_key = abcdefghijklmnop", "api_key = abcdefghijklmnop")
            .into_iter()
            .find(|finding| finding.category == SecurityCategory::ExposedSecret)
            .unwrap();
        assert_eq!(secret.severity, SecuritySeverity::Critical);
        assert_eq!(secret.confidence, 92);
    }

    #[test]
    fn evidence_redaction_removes_credential_values_and_url_user_info() {
        let token = ["ghp_", "ABCDEFGHIJKLMNOPQRSTUVWXYZ012345"].concat();
        let input = format!("api_key={token} https:{}//user:pass@example.invalid", "");
        let redacted = redact_possible_secrets(&input);
        assert!(!redacted.contains(&token));
        assert!(!redacted.contains("user:pass"));
        assert!(redacted.contains("[REDACTED]"));
    }
}
