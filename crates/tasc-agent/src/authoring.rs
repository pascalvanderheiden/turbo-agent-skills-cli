use crate::{
    evaluation::validate_skill_package,
    security::redact_possible_secrets,
    service::{SkillAdapter, StateRepository},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};
use tasc_core::{
    authoring::{
        LocalContext, SkillDraft, SkillDraftAnswers, SkillDraftStatus, WorkspaceDiagnostic,
        WorkspaceValidation, SKILL_DRAFT_SCHEMA_VERSION, WORKSPACE_VALIDATION_SCHEMA_VERSION,
    },
    categories::SKILL_CATEGORIES,
    evaluation::{EvaluationDiagnostic, EvaluationOutcome},
};

static DRAFT_SEQUENCE: AtomicU64 = AtomicU64::new(0);
const MAX_CONTEXT_BYTES: u64 = 256 * 1024;
const MAX_DRAFT_TOKENS: usize = 10_000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillDraftInput {
    pub workspace: PathBuf,
    pub name: String,
    pub purpose: String,
    pub description: String,
    pub triggers: String,
    pub non_goals: String,
    #[serde(default)]
    pub categories: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillDraftPlan {
    pub workspace_path: String,
    pub skill_path: String,
    pub local_context: Option<LocalContext>,
    pub questions: Vec<String>,
    pub categories: Vec<String>,
    pub preview: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SkillRemediationPlan {
    pub draft_id: String,
    pub workspace_path: String,
    pub skill_path: String,
    pub expected_content_digest: String,
    pub expected_result_digest: String,
    pub description: String,
    pub patch: String,
    pub recovery_path: String,
}

pub trait SkillDraftRepository {
    fn save(&self, draft: &SkillDraft) -> Result<(), String>;
    fn list(&self) -> Result<Vec<SkillDraft>, String>;
    fn get(&self, id: &str) -> Result<SkillDraft, String>;
}

#[derive(Debug, Clone)]
pub struct FsSkillDraftRepository {
    directory: PathBuf,
}

impl FsSkillDraftRepository {
    pub fn new(directory: PathBuf) -> Self {
        Self { directory }
    }
}

impl SkillDraftRepository for FsSkillDraftRepository {
    fn save(&self, draft: &SkillDraft) -> Result<(), String> {
        validate_draft_id(&draft.id)?;
        let bytes = serde_json::to_vec_pretty(draft)
            .map_err(|error| format!("cannot serialize Skill Draft: {error}"))?;
        atomic_write(&self.directory.join(format!("{}.json", draft.id)), &bytes)
    }

    fn list(&self) -> Result<Vec<SkillDraft>, String> {
        let mut drafts = Vec::new();
        let entries = match fs::read_dir(&self.directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(drafts),
            Err(error) => return Err(format!("cannot list Skill Drafts: {error}")),
        };
        for entry in entries {
            let entry = entry.map_err(|error| format!("cannot read Skill Draft entry: {error}"))?;
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path)
                .map_err(|error| format!("cannot inspect Skill Draft: {error}"))?;
            if metadata.file_type().is_symlink()
                || !metadata.is_file()
                || path.extension().is_none_or(|extension| extension != "json")
            {
                continue;
            }
            let bytes = fs::read(&path).map_err(|error| {
                format!("cannot read Skill Draft '{}': {error}", path.display())
            })?;
            let draft: SkillDraft = serde_json::from_slice(&bytes)
                .map_err(|error| format!("invalid Skill Draft '{}': {error}", path.display()))?;
            if path.file_stem().and_then(|stem| stem.to_str()) != Some(&draft.id) {
                return Err(format!(
                    "Skill Draft ID does not match its filename '{}'",
                    path.display()
                ));
            }
            drafts.push(draft);
        }
        drafts.sort_by(|left, right| left.created_at_unix_ms.cmp(&right.created_at_unix_ms));
        Ok(drafts)
    }

    fn get(&self, id: &str) -> Result<SkillDraft, String> {
        validate_draft_id(id)?;
        let path = self.directory.join(format!("{id}.json"));
        let metadata = fs::symlink_metadata(&path)
            .map_err(|error| format!("cannot inspect Skill Draft '{id}': {error}"))?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(format!("Skill Draft '{id}' is not a regular file"));
        }
        let bytes =
            fs::read(&path).map_err(|error| format!("cannot read Skill Draft '{id}': {error}"))?;
        let draft: SkillDraft = serde_json::from_slice(&bytes)
            .map_err(|error| format!("invalid Skill Draft '{id}': {error}"))?;
        if draft.id != id {
            return Err(format!("Skill Draft ID does not match '{id}'"));
        }
        Ok(draft)
    }
}

pub struct SkillAuthoringService<A, R, D> {
    adapter: A,
    state_repository: R,
    draft_repository: D,
}

impl<A: SkillAdapter, R: StateRepository, D: SkillDraftRepository> SkillAuthoringService<A, R, D> {
    pub fn new(adapter: A, state_repository: R, draft_repository: D) -> Self {
        Self {
            adapter,
            state_repository,
            draft_repository,
        }
    }

    pub fn plan(&self, input: &SkillDraftInput) -> Result<SkillDraftPlan, String> {
        let safe_input = redact_input(input);
        let workspace = validate_workspace(&safe_input.workspace)?;
        validate_input(&safe_input)?;
        let skill_path = workspace.join(&safe_input.name);
        self.ensure_not_installed(&skill_path, None)?;
        if fs::symlink_metadata(&skill_path).is_ok() {
            return Err(format!(
                "Skill Workspace destination '{}' already exists; choose a new name or inspect the existing draft.",
                skill_path.display()
            ));
        }
        let local_context = inspect_local_context()?;
        let preview = render_skill(&safe_input)?;
        Ok(SkillDraftPlan {
            workspace_path: workspace.to_string_lossy().into_owned(),
            skill_path: skill_path.to_string_lossy().into_owned(),
            questions: vec![
                "What concrete outcome should this skill produce, and what context must it gather first?"
                    .to_string(),
                "Give a specific trigger and a counterexample where this skill must stay inactive."
                    .to_string(),
                "What should the skill explicitly avoid, and how should it handle uncertainty?"
                    .to_string(),
            ],
            categories: safe_input.categories,
            preview,
            local_context,
        })
    }

    pub fn create(&self, input: &SkillDraftInput, approved: bool) -> Result<SkillDraft, String> {
        let input = redact_input(input);
        let plan = self.plan(&input)?;
        if !approved {
            return Err("Skill Draft creation requires explicit approval".to_string());
        }
        let now = now_unix_ms();
        let sequence = DRAFT_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let id = format!("draft-{now}-{}-{sequence}", std::process::id());
        let skill_path = PathBuf::from(&plan.skill_path);
        let draft = SkillDraft {
            schema_version: SKILL_DRAFT_SCHEMA_VERSION,
            id: id.clone(),
            name: input.name.clone(),
            purpose: input.purpose.clone(),
            workspace_path: plan.workspace_path,
            skill_path: skill_path.to_string_lossy().into_owned(),
            answers: SkillDraftAnswers {
                purpose: input.purpose.clone(),
                description: input.description.clone(),
                triggers: input.triggers.clone(),
                non_goals: input.non_goals.clone(),
            },
            categories: plan.categories,
            status: SkillDraftStatus::Incomplete,
            diagnostics: Vec::new(),
            quality_guidance: Vec::new(),
            local_context: plan.local_context,
            recovery_path: None,
            created_at_unix_ms: now,
            updated_at_unix_ms: now,
        };
        self.draft_repository.save(&draft)?;
        fs::create_dir(&skill_path).map_err(|error| {
            format!(
                "cannot create Skill Draft directory '{}': {error}",
                skill_path.display()
            )
        })?;
        write_draft_marker(&skill_path.join(".tasc-draft.json"), &draft)?;
        atomic_write(&skill_path.join("SKILL.md"), plan.preview.as_bytes())?;
        self.validate(&draft.id, &input.workspace)
    }

    pub fn validate(&self, id: &str, workspace: &Path) -> Result<SkillDraft, String> {
        let workspace = validate_workspace(workspace)?;
        let mut draft = self.draft_repository.get(id)?;
        if Path::new(&draft.workspace_path) != workspace {
            return Err("selected workspace does not match the Skill Draft workspace".to_string());
        }
        let skill_path = recorded_skill_path(&draft, &workspace)?;
        self.ensure_not_installed(&skill_path, Some(id))?;
        validate_draft_directory(&skill_path, &workspace)?;
        let (outcome, diagnostics) = validate_draft_package(&skill_path);
        draft.schema_version = SKILL_DRAFT_SCHEMA_VERSION;
        draft.quality_guidance = quality_guidance(&skill_path);
        draft.status = if outcome == EvaluationOutcome::Passed {
            SkillDraftStatus::Ready
        } else {
            SkillDraftStatus::Incomplete
        };
        draft.diagnostics = diagnostics
            .into_iter()
            .map(|diagnostic| WorkspaceDiagnostic {
                code: diagnostic.code,
                severity: diagnostic.severity,
                path: diagnostic.path,
                remediation: if outcome == EvaluationOutcome::Passed {
                    "No action required.".to_string()
                } else {
                    "Correct the indicated SKILL.md issue in this Skill Workspace, then validate again."
                        .to_string()
                },
                message: redact_possible_secrets(&diagnostic.message),
            })
            .collect();
        draft.updated_at_unix_ms = now_unix_ms();
        if draft.status == SkillDraftStatus::Ready {
            self.draft_repository.save(&draft)?;
            remove_draft_marker(&skill_path.join(".tasc-draft.json"))?;
        } else {
            write_draft_marker(&skill_path.join(".tasc-draft.json"), &draft)?;
            self.draft_repository.save(&draft)?;
        }
        Ok(draft)
    }

    pub fn validate_report(
        &self,
        id: &str,
        workspace: &Path,
    ) -> Result<WorkspaceValidation, String> {
        let draft = self.validate(id, workspace)?;
        Ok(WorkspaceValidation {
            schema_version: WORKSPACE_VALIDATION_SCHEMA_VERSION,
            draft_id: draft.id,
            status: draft.status,
            diagnostics: draft.diagnostics,
            quality_guidance: draft.quality_guidance,
            validated_at_unix_ms: draft.updated_at_unix_ms,
        })
    }

    pub fn resume(&self, id: &str, workspace: &Path) -> Result<SkillDraft, String> {
        let workspace = validate_workspace(workspace)?;
        let mut draft = self.draft_repository.get(id)?;
        if Path::new(&draft.workspace_path) != workspace {
            return Err("selected workspace does not match the Skill Draft workspace".to_string());
        }
        let skill_path = recorded_skill_path(&draft, &workspace)?;
        self.ensure_not_installed(&skill_path, Some(id))?;
        validate_draft_directory(&skill_path, &workspace)?;
        draft.schema_version = SKILL_DRAFT_SCHEMA_VERSION;
        Ok(draft)
    }

    pub fn list(&self, workspace: Option<&Path>) -> Result<Vec<SkillDraft>, String> {
        let drafts = self.draft_repository.list()?;
        let Some(workspace) = workspace else {
            return Ok(drafts);
        };
        let workspace = validate_workspace(workspace)?;
        Ok(drafts
            .into_iter()
            .filter(|draft| Path::new(&draft.workspace_path) == workspace)
            .collect())
    }

    pub fn plan_missing_description(
        &self,
        id: &str,
        workspace: &Path,
        description: &str,
    ) -> Result<SkillRemediationPlan, String> {
        let workspace = validate_workspace(workspace)?;
        let draft = self.draft_repository.get(id)?;
        if Path::new(&draft.workspace_path) != workspace {
            return Err("selected workspace does not match the Skill Draft workspace".to_string());
        }
        if draft.status != SkillDraftStatus::Incomplete {
            return Err("only an incomplete Skill Draft can be remediated".to_string());
        }
        validate_description(description)?;
        let skill_path = recorded_skill_path(&draft, &workspace)?;
        self.ensure_not_installed(&skill_path, Some(id))?;
        validate_draft_directory(&skill_path, &workspace)?;
        let skill_file = skill_path.join("SKILL.md");
        let metadata = fs::symlink_metadata(&skill_file)
            .map_err(|error| format!("cannot inspect draft SKILL.md: {error}"))?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err("draft SKILL.md must be a regular, non-symlink file".to_string());
        }
        let original = fs::read_to_string(&skill_file)
            .map_err(|error| format!("cannot read draft SKILL.md as UTF-8: {error}"))?;
        let repaired = add_missing_description(&original, description)?;
        let recovery_dir = workspace.join(".tasc-recovery");
        if let Ok(metadata) = fs::symlink_metadata(&recovery_dir) {
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err("Workspace recovery path must be a non-symlink directory".to_string());
            }
        }
        let sequence = DRAFT_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let recovery_path = recovery_dir.join(format!(
            "{}-{}-{sequence}.SKILL.md.before",
            draft.id,
            now_unix_ms()
        ));
        Ok(SkillRemediationPlan {
            draft_id: draft.id,
            workspace_path: workspace.to_string_lossy().into_owned(),
            skill_path: skill_path.to_string_lossy().into_owned(),
            expected_content_digest: digest(original.as_bytes()),
            expected_result_digest: digest(repaired.as_bytes()),
            description: description.trim().to_string(),
            patch: format!(
                "Add one frontmatter field: description: {}",
                serde_json::to_string(description.trim())
                    .map_err(|error| format!("cannot encode remediation description: {error}"))?
            ),
            recovery_path: recovery_path.to_string_lossy().into_owned(),
        })
    }

    pub fn apply_remediation(
        &self,
        plan: &SkillRemediationPlan,
        approved: bool,
    ) -> Result<SkillDraft, String> {
        if !approved {
            return Err("Workspace remediation requires explicit approval".to_string());
        }
        let workspace = validate_workspace(Path::new(&plan.workspace_path))?;
        let mut draft = self.draft_repository.get(&plan.draft_id)?;
        if Path::new(&draft.workspace_path) != workspace
            || Path::new(&draft.skill_path) != Path::new(&plan.skill_path)
        {
            return Err("Workspace remediation plan no longer matches the Skill Draft".to_string());
        }
        if draft.status != SkillDraftStatus::Incomplete {
            return Err("only an incomplete Skill Draft can be remediated".to_string());
        }
        let skill_path = recorded_skill_path(&draft, &workspace)?;
        self.ensure_not_installed(&skill_path, Some(&plan.draft_id))?;
        validate_draft_directory(&skill_path, &workspace)?;
        let skill_file = skill_path.join("SKILL.md");
        let metadata = fs::symlink_metadata(&skill_file)
            .map_err(|error| format!("cannot inspect draft SKILL.md: {error}"))?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err("draft SKILL.md must be a regular, non-symlink file".to_string());
        }
        let original = fs::read_to_string(&skill_file)
            .map_err(|error| format!("cannot read draft SKILL.md as UTF-8: {error}"))?;
        if digest(original.as_bytes()) != plan.expected_content_digest {
            return Err(
                "Skill Draft changed after the remediation plan; validate and plan again"
                    .to_string(),
            );
        }
        let repaired = add_missing_description(&original, &plan.description)?;
        if digest(repaired.as_bytes()) != plan.expected_result_digest {
            return Err("Workspace remediation plan has an invalid expected result".to_string());
        }
        let recovery_path = PathBuf::from(&plan.recovery_path);
        let recovery_dir = workspace.join(".tasc-recovery");
        let recovery_file = recovery_path.file_name().and_then(|name| name.to_str());
        if recovery_path.parent() != Some(recovery_dir.as_path())
            || !recovery_file.is_some_and(|name| {
                name.starts_with(&format!("{}-", draft.id)) && name.ends_with(".SKILL.md.before")
            })
        {
            return Err("Workspace recovery path is outside the selected Workspace".to_string());
        }
        match fs::symlink_metadata(&recovery_dir) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Err("Workspace recovery path must be a non-symlink directory".to_string())
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(&recovery_dir).map_err(|error| {
                    format!("cannot create Workspace recovery directory: {error}")
                })?;
            }
            Err(error) => {
                return Err(format!(
                    "cannot inspect Workspace recovery directory: {error}"
                ))
            }
        }
        if !fs::canonicalize(&recovery_dir)
            .map_err(|error| format!("cannot resolve Workspace recovery directory: {error}"))?
            .starts_with(&workspace)
        {
            return Err(
                "Workspace recovery directory resolves outside the selected Workspace".to_string(),
            );
        }
        let mut backup = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&recovery_path)
            .map_err(|error| format!("cannot preserve pre-remediation draft: {error}"))?;
        backup
            .write_all(original.as_bytes())
            .and_then(|_| backup.sync_all())
            .map_err(|error| format!("cannot preserve pre-remediation draft: {error}"))?;
        draft.recovery_path = Some(recovery_path.to_string_lossy().into_owned());
        atomic_write(&skill_file, repaired.as_bytes())?;
        let (outcome, diagnostics) = validate_draft_package(&skill_path);
        draft.quality_guidance = quality_guidance(&skill_path);
        draft.status = if outcome == EvaluationOutcome::Passed {
            SkillDraftStatus::Ready
        } else {
            atomic_write(&skill_file, original.as_bytes())?;
            SkillDraftStatus::Incomplete
        };
        draft.diagnostics = diagnostics
            .into_iter()
            .map(|diagnostic| WorkspaceDiagnostic {
                code: diagnostic.code,
                severity: diagnostic.severity,
                path: diagnostic.path,
                message: redact_possible_secrets(&diagnostic.message),
                remediation:
                    "Review the diagnostic and correct this Skill Draft in the selected Workspace."
                        .to_string(),
            })
            .collect();
        draft.updated_at_unix_ms = now_unix_ms();
        if draft.status == SkillDraftStatus::Ready {
            self.draft_repository.save(&draft)?;
            remove_draft_marker(&skill_path.join(".tasc-draft.json"))?;
        } else {
            write_draft_marker(&skill_path.join(".tasc-draft.json"), &draft)?;
            self.draft_repository.save(&draft)?;
        }
        Ok(draft)
    }

    fn ensure_not_installed(
        &self,
        skill_path: &Path,
        own_draft_id: Option<&str>,
    ) -> Result<(), String> {
        let candidate = canonical_or_absolute(skill_path);
        let state = self.state_repository.load()?;
        let discovered = self.adapter.discover()?;
        for installation in &state.installations {
            let installation_path = canonical_or_absolute(Path::new(&installation.path));
            if candidate == installation_path || candidate.starts_with(&installation_path) {
                return Err("Skill Workspace destination overlaps an Installation; installed copies are immutable authoring targets.".to_string());
            }
        }
        for skill in discovered {
            let installed_path = canonical_or_absolute(Path::new(&skill.path));
            if own_draft_id.is_some() && candidate == installed_path {
                continue;
            }
            if candidate == installed_path || candidate.starts_with(&installed_path) {
                return Err("Skill Workspace destination overlaps a discovered installed skill; choose a separate workspace.".to_string());
            }
        }
        for target_path in self.adapter.target_paths() {
            let target = canonical_or_absolute(&target_path);
            if candidate == target || candidate.starts_with(&target) {
                return Err("Skill Workspace destination overlaps an Adapter Target; author the Skill Draft in a separate Workspace.".to_string());
            }
        }
        Ok(())
    }
}

fn validate_workspace(path: &Path) -> Result<PathBuf, String> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        format!(
            "explicit Skill Workspace '{}' is unavailable: {error}",
            path.display()
        )
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("Skill Workspace must be an existing, non-symlink directory".to_string());
    }
    fs::canonicalize(path).map_err(|error| {
        format!(
            "cannot resolve Skill Workspace '{}': {error}",
            path.display()
        )
    })
}

fn recorded_skill_path(draft: &SkillDraft, workspace: &Path) -> Result<PathBuf, String> {
    if !valid_skill_name(&draft.name) {
        return Err("Skill Draft has an invalid recorded name".to_string());
    }
    let expected = workspace.join(&draft.name);
    if Path::new(&draft.skill_path) != expected {
        return Err("Skill Draft path does not match its recorded Workspace and name".to_string());
    }
    Ok(expected)
}

fn validate_draft_directory(path: &Path, workspace: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("cannot inspect Skill Draft directory: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("Skill Draft directory must be a regular, non-symlink directory".to_string());
    }
    if !fs::canonicalize(path)
        .map_err(|error| format!("cannot resolve Skill Draft directory: {error}"))?
        .starts_with(workspace)
    {
        return Err("Skill Draft directory resolves outside its selected Workspace".to_string());
    }
    Ok(())
}

fn write_draft_marker(path: &Path, draft: &SkillDraft) -> Result<(), String> {
    if let Ok(metadata) = fs::symlink_metadata(path) {
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err("draft marker must be a regular, non-symlink file".to_string());
        }
    }
    let marker = serde_json::to_vec(&serde_json::json!({ "draftId": draft.id }))
        .map_err(|error| format!("cannot serialize draft marker: {error}"))?;
    atomic_write(path, &marker)
}

fn remove_draft_marker(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            Err("draft marker must be a regular, non-symlink file".to_string())
        }
        Ok(_) => fs::remove_file(path)
            .map_err(|error| format!("cannot finalize Skill Draft marker: {error}")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("cannot inspect Skill Draft marker: {error}")),
    }
}

fn validate_input(input: &SkillDraftInput) -> Result<(), String> {
    if !valid_skill_name(&input.name) {
        return Err(
            "skill name must use 1-64 lowercase letters, numbers, and single hyphens".to_string(),
        );
    }
    if input.purpose.trim().is_empty() || input.purpose.chars().count() > 8192 {
        return Err("skill purpose must contain 1-8192 characters".to_string());
    }
    validate_description(&input.description)?;
    if input.triggers.trim().is_empty() || input.triggers.chars().count() > 4096 {
        return Err("skill trigger guidance must contain 1-4096 characters".to_string());
    }
    if input.non_goals.trim().is_empty() || input.non_goals.chars().count() > 4096 {
        return Err("skill non-goals must contain 1-4096 characters".to_string());
    }
    let categories = input
        .categories
        .iter()
        .collect::<std::collections::HashSet<_>>();
    if categories.len() != input.categories.len()
        || input
            .categories
            .iter()
            .any(|category| !SKILL_CATEGORIES.iter().any(|known| known.id == category))
    {
        return Err(
            "skill categories must be unique IDs from the descriptive category catalog".to_string(),
        );
    }
    Ok(())
}

fn redact_input(input: &SkillDraftInput) -> SkillDraftInput {
    SkillDraftInput {
        workspace: input.workspace.clone(),
        name: input.name.clone(),
        purpose: redact_answer(&input.purpose),
        description: redact_answer(&input.description),
        triggers: redact_answer(&input.triggers),
        non_goals: redact_answer(&input.non_goals),
        categories: input.categories.clone(),
    }
}

fn redact_answer(value: &str) -> String {
    let mut in_private_key = false;
    let mut in_putty_key = false;
    let mut redacted_lines = Vec::new();
    for line in value.lines() {
        let lower = line.to_ascii_lowercase();
        if in_putty_key {
            if lower.starts_with("private-mac:") {
                in_putty_key = false;
            }
            continue;
        }
        if lower.starts_with("putty-user-key-file-") {
            redacted_lines.push("[REDACTED PRIVATE KEY MATERIAL]".to_string());
            in_putty_key = true;
            continue;
        }
        if in_private_key {
            if lower.contains("-----end") && lower.contains("private key") {
                in_private_key = false;
            }
            continue;
        }
        if lower.contains("-----begin") && lower.contains("private key") {
            redacted_lines.push("[REDACTED PRIVATE KEY MATERIAL]".to_string());
            in_private_key = !(lower.contains("-----end") && lower.contains("private key"));
            continue;
        }
        redacted_lines.push(redact_possible_secrets(line));
    }
    redacted_lines.join("\n")
}

fn validate_description(description: &str) -> Result<(), String> {
    if description.trim().is_empty() || description.chars().count() > 1024 {
        return Err("skill description must contain 1-1024 characters".to_string());
    }
    Ok(())
}

fn validate_draft_package(path: &Path) -> (EvaluationOutcome, Vec<EvaluationDiagnostic>) {
    let (mut outcome, mut diagnostics) = validate_skill_package(path);
    let skill_file = path.join("SKILL.md");
    if !fs::symlink_metadata(path)
        .is_ok_and(|metadata| metadata.is_dir() && !metadata.file_type().is_symlink())
    {
        return (outcome, diagnostics);
    }
    let metadata = match fs::symlink_metadata(&skill_file) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => metadata,
        _ => return (outcome, diagnostics),
    };
    if metadata.len() > 1_048_576 {
        return (outcome, diagnostics);
    }
    let contents = match fs::read_to_string(&skill_file) {
        Ok(contents) => contents,
        Err(_) => return (outcome, diagnostics),
    };
    let token_count = contents.split_whitespace().count();
    if token_count > MAX_DRAFT_TOKENS {
        diagnostics.push(authoring_diagnostic(
            "token_limit_exceeded",
            format!(
                "SKILL.md contains {token_count} whitespace-delimited tokens; the deterministic limit is {MAX_DRAFT_TOKENS}."
            ),
            &skill_file,
            "Reduce the draft to 10,000 or fewer whitespace-delimited tokens.",
        ));
    }
    if !has_trigger_guidance(&contents) {
        diagnostics.push(authoring_diagnostic(
            "trigger_guidance_missing",
            "SKILL.md must include a non-empty 'When to use', 'Triggers', or 'Activation' section."
                .to_string(),
            &skill_file,
            "Add a heading and concrete activation guidance before validating again.",
        ));
    }
    diagnostics.extend(validate_local_links(path, &skill_file, &contents));
    if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == "error")
    {
        outcome = EvaluationOutcome::Failed;
    }
    (outcome, diagnostics)
}

fn quality_guidance(path: &Path) -> Vec<WorkspaceDiagnostic> {
    let skill_file = path.join("SKILL.md");
    let Ok(contents) = fs::read_to_string(&skill_file) else {
        return Vec::new();
    };
    if contents.lines().any(|line| {
        line.trim_start_matches('#')
            .trim()
            .eq_ignore_ascii_case("examples")
    }) {
        return Vec::new();
    }
    vec![WorkspaceDiagnostic {
        code: "quality_examples_suggested".to_string(),
        severity: "suggestion".to_string(),
        path: Some(skill_file.to_string_lossy().into_owned()),
        message: "Consider adding a concise example to clarify expected inputs and outputs; this is optional and does not affect readiness.".to_string(),
        remediation: "Add examples only when they improve clarity for this Skill.".to_string(),
    }]
}

fn has_trigger_guidance(contents: &str) -> bool {
    let mut in_trigger_section = false;
    let mut has_guidance = false;
    for line in contents.lines() {
        let trimmed = line.trim();
        if let Some(heading) = trimmed.strip_prefix('#') {
            if let Some(heading) = heading.trim().strip_prefix('#') {
                let heading = heading.trim().to_ascii_lowercase();
                in_trigger_section =
                    matches!(heading.as_str(), "when to use" | "triggers" | "activation");
                continue;
            }
            in_trigger_section = false;
        }
        if in_trigger_section && !trimmed.is_empty() {
            has_guidance = true;
        }
    }
    has_guidance
}

fn validate_local_links(
    package: &Path,
    skill_file: &Path,
    contents: &str,
) -> Vec<EvaluationDiagnostic> {
    let mut diagnostics = Vec::new();
    for segment in contents.split("](").skip(1) {
        let Some(target) = segment.split(')').next().map(str::trim) else {
            continue;
        };
        let target = target.split_whitespace().next().unwrap_or_default();
        if target.is_empty()
            || target.starts_with('#')
            || target.starts_with("https://")
            || target.starts_with("http://")
            || target.starts_with("mailto:")
        {
            continue;
        }
        let local_target = target.split(['?', '#']).next().unwrap_or(target);
        let relative = Path::new(local_target);
        if relative.is_absolute()
            || relative.components().any(|component| {
                matches!(
                    component,
                    std::path::Component::ParentDir
                        | std::path::Component::RootDir
                        | std::path::Component::Prefix(_)
                )
            })
        {
            diagnostics.push(authoring_diagnostic(
                "workspace_link_outside_package",
                format!("Local link '{target}' escapes the Skill package."),
                skill_file,
                "Use a relative resource path contained in this Skill package.",
            ));
            continue;
        }
        match inspect_package_link(package, relative) {
            Ok(()) => {}
            Err(LinkProblem::Symlink) => diagnostics.push(authoring_diagnostic(
                "workspace_link_symlink",
                format!("Local link '{target}' points to a symbolic link."),
                skill_file,
                "Replace the link with a regular file inside the Skill package.",
            )),
            Err(LinkProblem::Missing) => diagnostics.push(authoring_diagnostic(
                "workspace_link_missing",
                format!("Local link '{target}' does not resolve inside the Skill package."),
                skill_file,
                "Create the linked resource in this package or correct the link target.",
            )),
            Err(LinkProblem::Unreadable(error)) => diagnostics.push(authoring_diagnostic(
                "workspace_link_unreadable",
                format!("Cannot inspect local link '{target}': {error}"),
                skill_file,
                "Check Workspace permissions and validate again.",
            )),
        }
    }
    diagnostics
}

enum LinkProblem {
    Missing,
    Symlink,
    Unreadable(std::io::Error),
}

fn inspect_package_link(package: &Path, relative: &Path) -> Result<(), LinkProblem> {
    let mut current = package.to_path_buf();
    for component in relative.components() {
        let std::path::Component::Normal(part) = component else {
            continue;
        };
        current.push(part);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(LinkProblem::Symlink);
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(LinkProblem::Missing);
            }
            Err(error) => return Err(LinkProblem::Unreadable(error)),
        }
    }
    Ok(())
}

fn authoring_diagnostic(
    code: &str,
    message: String,
    path: &Path,
    remediation: &str,
) -> EvaluationDiagnostic {
    EvaluationDiagnostic {
        code: code.to_string(),
        severity: "error".to_string(),
        message: format!("{message} Remediation: {remediation}"),
        path: Some(path.to_string_lossy().into_owned()),
    }
}

fn valid_skill_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 64
        && (bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit())
        && (bytes[bytes.len() - 1].is_ascii_lowercase() || bytes[bytes.len() - 1].is_ascii_digit())
        && bytes
            .windows(2)
            .all(|pair| pair[0] != b'-' || pair[1] != b'-')
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
}

fn render_skill(input: &SkillDraftInput) -> Result<String, String> {
    let description = serde_json::to_string(input.description.trim())
        .map_err(|error| format!("cannot encode skill description: {error}"))?;
    let metadata = if input.categories.is_empty() {
        String::new()
    } else {
        let categories = serde_json::to_string(&input.categories)
            .map_err(|error| format!("cannot encode skill categories: {error}"))?;
        format!("metadata:\n  tascCategories: {categories}\n")
    };
    Ok(format!(
        "---\nname: {}\ndescription: {description}\n{metadata}---\n\n# {}\n\n## Purpose\n\n{}\n\n## When to use\n\n{}\n\n## Instructions\n\n1. Confirm the request matches the purpose above.\n2. Gather the minimum necessary context and state assumptions.\n3. Provide a clear, actionable result; explain uncertainty instead of inventing facts.\n\n## Non-goals\n\n{}\n",
        input.name, input.name, input.purpose.trim(), input.triggers.trim(), input.non_goals.trim()
    ))
}

fn inspect_local_context() -> Result<Option<LocalContext>, String> {
    let current = std::env::current_dir()
        .map_err(|error| format!("cannot inspect current directory for local context: {error}"))?;
    for directory in current.ancestors() {
        let path = directory.join("CONTEXT.md");
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(format!(
                    "cannot inspect local context '{}': {error}",
                    path.display()
                ))
            }
        };
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            continue;
        }
        if metadata.len() > MAX_CONTEXT_BYTES {
            return Err(format!(
                "local context '{}' exceeds the 256 KiB inspection limit",
                path.display()
            ));
        }
        let contents = fs::read_to_string(&path)
            .map_err(|error| format!("cannot read local context '{}': {error}", path.display()))?;
        let headings = contents
            .lines()
            .filter_map(|line| line.trim().strip_prefix('#'))
            .map(str::trim)
            .filter(|heading| !heading.is_empty())
            .take(32)
            .map(str::to_string)
            .collect();
        return Ok(Some(LocalContext {
            path: path.to_string_lossy().into_owned(),
            digest: digest(contents.as_bytes()),
            headings,
        }));
    }
    Ok(None)
}

fn add_missing_description(contents: &str, description: &str) -> Result<String, String> {
    let mut lines = contents.lines().map(str::to_string).collect::<Vec<_>>();
    if lines.first().map(String::as_str) != Some("---") {
        return Err("bounded remediation supports only a valid frontmatter block with a missing description".to_string());
    }
    let close = lines
        .iter()
        .enumerate()
        .skip(1)
        .find(|(_, line)| line.trim() == "---")
        .map(|(index, _)| index)
        .ok_or_else(|| "bounded remediation requires closed frontmatter".to_string())?;
    if lines[1..close]
        .iter()
        .any(|line| line.trim_start().starts_with("description:"))
    {
        return Err("bounded remediation only adds a missing description field".to_string());
    }
    let name = lines[1..close]
        .iter()
        .position(|line| line.trim_start().starts_with("name:"))
        .map(|index| index + 1)
        .ok_or_else(|| "bounded remediation requires a name field".to_string())?;
    let quoted = serde_json::to_string(description.trim())
        .map_err(|error| format!("cannot encode remediation description: {error}"))?;
    lines.insert(name + 1, format!("description: {quoted}"));
    Ok(format!("{}\n", lines.join("\n")))
}

fn canonical_or_absolute(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(path)
    };
    let mut existing = absolute.clone();
    let mut unresolved = Vec::new();
    while fs::canonicalize(&existing).is_err() {
        let Some(component) = existing.file_name() else {
            return absolute;
        };
        unresolved.push(component.to_os_string());
        if !existing.pop() {
            return absolute;
        }
    }
    let mut canonical = fs::canonicalize(existing).unwrap_or(absolute);
    for component in unresolved.iter().rev() {
        canonical.push(component);
    }
    canonical
}

fn validate_draft_id(id: &str) -> Result<(), String> {
    if id.is_empty()
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        return Err("invalid Skill Draft ID".to_string());
    }
    Ok(())
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)
        .map_err(|error| format!("cannot create directory '{}': {error}", parent.display()))?;
    let sequence = DRAFT_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temp_path = parent.join(format!(".tasc-{}-{sequence}.tmp", std::process::id()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp_path)
        .map_err(|error| format!("cannot create temporary file: {error}"))?;
    if let Err(error) = file.write_all(bytes).and_then(|_| file.sync_all()) {
        let _ = fs::remove_file(&temp_path);
        return Err(format!("cannot write '{}': {error}", path.display()));
    }
    drop(file);
    if let Err(error) = fs::rename(&temp_path, path) {
        let _ = fs::remove_file(&temp_path);
        return Err(format!("cannot finalize '{}': {error}", path.display()));
    }
    Ok(())
}

fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn now_unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::DiscoveredSkill;
    use tasc_core::LocalState;

    struct NoSkills;

    impl SkillAdapter for NoSkills {
        fn discover(&self) -> Result<Vec<DiscoveredSkill>, String> {
            Ok(Vec::new())
        }
    }

    struct MemoryState(LocalState);

    impl StateRepository for MemoryState {
        fn load(&self) -> Result<LocalState, String> {
            Ok(self.0.clone())
        }

        fn save(&self, state: &LocalState) -> Result<(), String> {
            let _ = state;
            Ok(())
        }
    }

    fn test_service(
        root: &Path,
    ) -> SkillAuthoringService<NoSkills, MemoryState, FsSkillDraftRepository> {
        SkillAuthoringService::new(
            NoSkills,
            MemoryState(LocalState::default()),
            FsSkillDraftRepository::new(root.join("draft-records")),
        )
    }

    fn test_input(workspace: &Path) -> SkillDraftInput {
        SkillDraftInput {
            workspace: workspace.to_path_buf(),
            name: "authoring-test".to_string(),
            purpose: "Test Workspace authoring.".to_string(),
            description: "Test deterministic authoring.".to_string(),
            triggers: "When testing authoring.".to_string(),
            non_goals: "Do not edit installed skills.".to_string(),
            categories: vec!["testing".to_string()],
        }
    }

    fn test_root() -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "tasc-authoring-test-{}-{}",
            std::process::id(),
            DRAFT_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn remediation_rejects_a_draft_changed_after_approval_plan() {
        let root = test_root();
        let workspace = root.join("workspace");
        fs::create_dir(&workspace).unwrap();
        let service = test_service(&root);
        let input = test_input(&workspace);
        let draft = service.create(&input, true).unwrap();
        let skill_file = Path::new(&draft.skill_path).join("SKILL.md");
        let invalid = "---\nname: authoring-test\n---\n## When to use\n\nWhen testing.\n";
        fs::write(&skill_file, invalid).unwrap();
        service.validate(&draft.id, &workspace).unwrap();
        let plan = service
            .plan_missing_description(&draft.id, &workspace, "A valid description.")
            .unwrap();
        let changed = format!("{invalid}\nChanged while awaiting approval.\n");
        fs::write(&skill_file, &changed).unwrap();

        let error = service.apply_remediation(&plan, true).unwrap_err();
        assert!(error.contains("changed after the remediation plan"));
        assert_eq!(fs::read_to_string(&skill_file).unwrap(), changed);
        assert!(!workspace.join(".tasc-recovery").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failed_remediation_restores_the_original_and_keeps_a_recovery_copy() {
        let root = test_root();
        let workspace = root.join("workspace");
        fs::create_dir(&workspace).unwrap();
        let service = test_service(&root);
        let input = test_input(&workspace);
        let draft = service.create(&input, true).unwrap();
        let skill_file = Path::new(&draft.skill_path).join("SKILL.md");
        let invalid = "---\nname: authoring-test\n---\nMissing activation guidance.\n";
        fs::write(&skill_file, invalid).unwrap();
        service.validate(&draft.id, &workspace).unwrap();
        let plan = service
            .plan_missing_description(&draft.id, &workspace, "A valid description.")
            .unwrap();

        let result = service.apply_remediation(&plan, true).unwrap();
        assert_eq!(result.status, SkillDraftStatus::Incomplete);
        assert_eq!(fs::read_to_string(&skill_file).unwrap(), invalid);
        assert_eq!(
            fs::read_to_string(result.recovery_path.unwrap()).unwrap(),
            invalid
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn drafts_resume_with_redacted_structured_answers_and_nonblocking_quality_guidance() {
        let root = test_root();
        let workspace = root.join("workspace");
        fs::create_dir(&workspace).unwrap();
        let service = test_service(&root);
        let mut input = test_input(&workspace);
        input.purpose = "Use api_key=credential-value-123 to test.".to_string();
        let draft = service.create(&input, true).unwrap();

        assert_eq!(draft.status, SkillDraftStatus::Ready);
        assert_eq!(draft.answers.purpose, "Use api_key= [REDACTED]");
        assert_eq!(draft.categories, vec!["testing"]);
        assert!(draft.quality_guidance.iter().any(|item| {
            item.code == "quality_examples_suggested" && item.severity == "suggestion"
        }));
        assert!(service
            .resume(&draft.id, &workspace)
            .unwrap()
            .answers
            .eq(&draft.answers));

        let saved = fs::read_to_string(
            root.join("draft-records")
                .join(format!("{}.json", draft.id)),
        )
        .unwrap();
        assert!(!saved.contains("credential-value-123"));
        assert!(!saved.contains("transcript"));
        let skill = fs::read_to_string(Path::new(&draft.skill_path).join("SKILL.md")).unwrap();
        assert!(!skill.contains("credential-value-123"));
        assert!(skill.contains("tascCategories: [\"testing\"]"));
        assert!(service.resume(&draft.id, &root).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn multiline_private_key_material_is_redacted_from_draft_and_skill_content() {
        let root = test_root();
        let workspace = root.join("workspace");
        fs::create_dir(&workspace).unwrap();
        let service = test_service(&root);
        let mut input = test_input(&workspace);
        let private_material = "unique-private-key-payload-for-regression";
        input.purpose = format!(
            "Use this key:\n-----BEGIN RSA PRIVATE KEY-----\n{private_material}\n-----END RSA PRIVATE KEY-----\nfor local testing.\nPuTTY-User-Key-File-3: ssh-ed25519\nEncryption: none\nPrivate-Lines: 1\n{private_material}\nPrivate-MAC: 0123456789"
        );

        let draft = service.create(&input, true).unwrap();
        assert!(!draft.answers.purpose.contains(private_material));
        assert!(draft
            .answers
            .purpose
            .contains("[REDACTED PRIVATE KEY MATERIAL]"));
        assert!(!draft.answers.purpose.contains("PuTTY-User-Key-File"));
        let skill_file = fs::read_to_string(Path::new(&draft.skill_path).join("SKILL.md")).unwrap();
        assert!(!skill_file.contains(private_material));
        let saved = fs::read_to_string(
            root.join("draft-records")
                .join(format!("{}.json", draft.id)),
        )
        .unwrap();
        assert!(!saved.contains(private_material));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn normative_validation_failures_block_readiness_separately_from_quality_guidance() {
        let root = test_root();
        let workspace = root.join("workspace");
        fs::create_dir(&workspace).unwrap();
        let service = test_service(&root);
        let draft = service.create(&test_input(&workspace), true).unwrap();
        let skill_file = Path::new(&draft.skill_path).join("SKILL.md");
        fs::write(
            &skill_file,
            "---\nname: authoring-test\n---\n## When to use\n\nWhen testing.\n",
        )
        .unwrap();

        let validated = service.validate(&draft.id, &workspace).unwrap();
        assert_eq!(validated.status, SkillDraftStatus::Incomplete);
        assert!(validated
            .diagnostics
            .iter()
            .any(|item| item.severity == "error"));
        assert!(validated
            .quality_guidance
            .iter()
            .all(|item| item.severity != "error"));
        fs::remove_dir_all(root).unwrap();
    }
}
