use crate::{
    evaluation::validate_skill_package,
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
        LocalContext, SkillDraft, SkillDraftStatus, WorkspaceDiagnostic,
    },
    evaluation::EvaluationOutcome,
};

static DRAFT_SEQUENCE: AtomicU64 = AtomicU64::new(0);
const MAX_CONTEXT_BYTES: u64 = 256 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillDraftInput {
    pub workspace: PathBuf,
    pub name: String,
    pub purpose: String,
    pub description: String,
    pub triggers: String,
    pub non_goals: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillDraftPlan {
    pub workspace_path: String,
    pub skill_path: String,
    pub local_context: Option<LocalContext>,
    pub questions: Vec<String>,
    pub preview: String,
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
            let bytes = fs::read(&path)
                .map_err(|error| format!("cannot read Skill Draft '{}': {error}", path.display()))?;
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
        let workspace = validate_workspace(&input.workspace)?;
        validate_input(input)?;
        let skill_path = workspace.join(&input.name);
        self.ensure_not_installed(&skill_path)?;
        if fs::symlink_metadata(&skill_path).is_ok() {
            return Err(format!(
                "Skill Workspace destination '{}' already exists; choose a new name or inspect the existing draft.",
                skill_path.display()
            ));
        }
        let local_context = inspect_local_context()?;
        Ok(SkillDraftPlan {
            workspace_path: workspace.to_string_lossy().into_owned(),
            skill_path: skill_path.to_string_lossy().into_owned(),
            questions: Vec::new(),
            preview: render_skill(input)?,
            local_context,
        })
    }

    pub fn create(
        &self,
        input: &SkillDraftInput,
        approved: bool,
    ) -> Result<SkillDraft, String> {
        let plan = self.plan(input)?;
        if !approved {
            return Err("Skill Draft creation requires explicit approval".to_string());
        }
        let now = now_unix_ms();
        let sequence = DRAFT_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let id = format!("draft-{now}-{}-{sequence}", std::process::id());
        let skill_path = PathBuf::from(&plan.skill_path);
        let draft = SkillDraft {
            schema_version: 1,
            id: id.clone(),
            name: input.name.clone(),
            purpose: input.purpose.clone(),
            workspace_path: plan.workspace_path,
            skill_path: skill_path.to_string_lossy().into_owned(),
            status: SkillDraftStatus::Incomplete,
            diagnostics: Vec::new(),
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
        let marker = serde_json::to_vec(&serde_json::json!({ "draftId": id }))
            .map_err(|error| format!("cannot serialize draft marker: {error}"))?;
        atomic_write(&skill_path.join(".tasc-draft.json"), &marker)?;
        atomic_write(
            &skill_path.join("SKILL.md"),
            plan.preview.as_bytes(),
        )?;
        self.validate(&draft.id, &input.workspace)
    }

    pub fn validate(
        &self,
        id: &str,
        workspace: &Path,
    ) -> Result<SkillDraft, String> {
        let workspace = validate_workspace(workspace)?;
        let mut draft = self.draft_repository.get(id)?;
        if Path::new(&draft.workspace_path) != workspace {
            return Err("selected workspace does not match the Skill Draft workspace".to_string());
        }
        let skill_path = PathBuf::from(&draft.skill_path);
        if !skill_path.starts_with(&workspace) {
            return Err("Skill Draft path is outside its recorded workspace".to_string());
        }
        let (outcome, diagnostics) = validate_skill_package(&skill_path);
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
                message: diagnostic.message,
            })
            .collect();
        draft.updated_at_unix_ms = now_unix_ms();
        let marker = skill_path.join(".tasc-draft.json");
        if draft.status == SkillDraftStatus::Ready {
            if let Ok(metadata) = fs::symlink_metadata(&marker) {
                if metadata.file_type().is_symlink() || !metadata.is_file() {
                    return Err("draft marker must be a regular, non-symlink file".to_string());
                }
                fs::remove_file(&marker)
                    .map_err(|error| format!("cannot finalize Skill Draft marker: {error}"))?;
            }
        } else {
            let marker_bytes = serde_json::to_vec(&serde_json::json!({ "draftId": draft.id }))
                .map_err(|error| format!("cannot serialize draft marker: {error}"))?;
            atomic_write(&marker, &marker_bytes)?;
        }
        self.draft_repository.save(&draft)?;
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

    pub fn remediate_missing_description(
        &self,
        id: &str,
        workspace: &Path,
        description: &str,
        approved: bool,
    ) -> Result<SkillDraft, String> {
        let workspace = validate_workspace(workspace)?;
        let mut draft = self.draft_repository.get(id)?;
        if Path::new(&draft.workspace_path) != workspace {
            return Err("selected workspace does not match the Skill Draft workspace".to_string());
        }
        if draft.status != SkillDraftStatus::Incomplete {
            return Err("only an incomplete Skill Draft can be remediated".to_string());
        }
        if !approved {
            return Err("Workspace remediation requires explicit approval".to_string());
        }
        validate_description(description)?;
        let skill_path = PathBuf::from(&draft.skill_path);
        if !skill_path.starts_with(&workspace) {
            return Err("Skill Draft path is outside its recorded workspace".to_string());
        }
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
        fs::create_dir_all(&recovery_dir)
            .map_err(|error| format!("cannot create Workspace recovery directory: {error}"))?;
        let recovery_path = recovery_dir.join(format!("{}.SKILL.md.before", draft.id));
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
        let (outcome, diagnostics) = validate_skill_package(&skill_path);
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
                message: diagnostic.message,
                remediation: "Review the diagnostic and correct this Skill Draft in the selected Workspace."
                    .to_string(),
            })
            .collect();
        draft.updated_at_unix_ms = now_unix_ms();
        if draft.status == SkillDraftStatus::Ready {
            let marker = skill_path.join(".tasc-draft.json");
            if marker.exists() {
                fs::remove_file(&marker)
                    .map_err(|error| format!("cannot finalize Skill Draft marker: {error}"))?;
            }
        }
        self.draft_repository.save(&draft)?;
        Ok(draft)
    }

    fn ensure_not_installed(&self, skill_path: &Path) -> Result<(), String> {
        let state = self.state_repository.load()?;
        let discovered = self.adapter.discover()?;
        for installation in &state.installations {
            let installation_path = canonical_or_absolute(Path::new(&installation.path));
            let candidate = canonical_or_absolute(skill_path);
            if candidate == installation_path || candidate.starts_with(&installation_path) {
                return Err("Skill Workspace destination overlaps an Installation; installed copies are immutable authoring targets.".to_string());
            }
        }
        for skill in discovered {
            let installed_path = canonical_or_absolute(Path::new(&skill.path));
            let candidate = canonical_or_absolute(skill_path);
            if candidate == installed_path || candidate.starts_with(&installed_path) {
                return Err("Skill Workspace destination overlaps a discovered installed skill; choose a separate workspace.".to_string());
            }
        }
        Ok(())
    }
}

fn validate_workspace(path: &Path) -> Result<PathBuf, String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("explicit Skill Workspace '{}' is unavailable: {error}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("Skill Workspace must be an existing, non-symlink directory".to_string());
    }
    fs::canonicalize(path)
        .map_err(|error| format!("cannot resolve Skill Workspace '{}': {error}", path.display()))
}

fn validate_input(input: &SkillDraftInput) -> Result<(), String> {
    if !valid_skill_name(&input.name) {
        return Err("skill name must use 1-64 lowercase letters, numbers, and single hyphens".to_string());
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
    Ok(())
}

fn validate_description(description: &str) -> Result<(), String> {
    if description.trim().is_empty() || description.chars().count() > 1024 {
        return Err("skill description must contain 1-1024 characters".to_string());
    }
    Ok(())
}

fn valid_skill_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 64
        && (bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit())
        && (bytes[bytes.len() - 1].is_ascii_lowercase() || bytes[bytes.len() - 1].is_ascii_digit())
        && bytes.windows(2).all(|pair| pair[0] != b'-' || pair[1] != b'-')
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
}

fn render_skill(input: &SkillDraftInput) -> Result<String, String> {
    let description = serde_json::to_string(input.description.trim())
        .map_err(|error| format!("cannot encode skill description: {error}"))?;
    Ok(format!(
        "---\nname: {}\ndescription: {description}\n---\n\n# {}\n\n## Purpose\n\n{}\n\n## When to use\n\n{}\n\n## Instructions\n\n1. Confirm the request matches the purpose above.\n2. Gather the minimum necessary context and state assumptions.\n3. Provide a clear, actionable result; explain uncertainty instead of inventing facts.\n\n## Non-goals\n\n{}\n",
        input.name,
        input.name,
        input.purpose.trim(),
        input.triggers.trim(),
        input.non_goals.trim()
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
            Err(error) => return Err(format!("cannot inspect local context '{}': {error}", path.display())),
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
    fs::canonicalize(path).unwrap_or_else(|_| {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()
                .unwrap_or_default()
                .join(path)
        }
    })
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
    let temp_path = parent.join(format!(
        ".tasc-{}-{sequence}.tmp",
        std::process::id()
    ));
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
