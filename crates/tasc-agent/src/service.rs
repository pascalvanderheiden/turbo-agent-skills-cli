use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
use tasc_core::{
    AdapterTarget, Installation, InstallationStatus, Inventory, InventoryItem, LocalState,
    ProfileScope, ProvenanceConfidence, ProvenanceEvidence, ProvenanceKind, SkillIdentity,
    StateStatus, TascProfile, INVENTORY_SCHEMA_VERSION, LOCAL_STATE_VERSION,
};

static TEMP_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub trait SkillAdapter {
    fn discover(&self) -> Result<Vec<DiscoveredSkill>, String>;
}

pub trait StateRepository {
    fn load(&self) -> Result<LocalState, String>;
    fn save(&self, state: &LocalState) -> Result<(), String>;
    fn profile(&self) -> TascProfile {
        TascProfile {
            id: "user".to_string(),
            scope: ProfileScope::User,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredSkill {
    pub identity: SkillIdentity,
    pub target: AdapterTarget,
    pub path: String,
    pub description: String,
    pub source_revision: Option<String>,
    pub content_digest: String,
}

pub struct AgentService<A, R> {
    adapter: A,
    repository: R,
}

impl<A: SkillAdapter, R: StateRepository> AgentService<A, R> {
    pub fn new(adapter: A, repository: R) -> Self {
        Self {
            adapter,
            repository,
        }
    }

    pub fn inventory(&self) -> Result<Inventory, String> {
        let state = self.repository.load()?;
        validate_state_version(&state)?;
        self.inventory_from_state(&state)
    }

    pub fn inventory_from_state(&self, state: &LocalState) -> Result<Inventory, String> {
        validate_state_version(state)?;
        let mut inventory = self.inventory_for(state)?;
        inventory.profile = Some(self.repository.profile());
        Ok(inventory)
    }

    fn inventory_for(&self, state: &LocalState) -> Result<Inventory, String> {
        let mut discovered = self.adapter.discover()?;
        discovered.sort_by(|left, right| {
            (
                &left.identity.id,
                &left.target.id,
                &left.path,
                &left.content_digest,
            )
                .cmp(&(
                    &right.identity.id,
                    &right.target.id,
                    &right.path,
                    &right.content_digest,
                ))
        });
        discovered.dedup_by(|left, right| {
            left.identity.id == right.identity.id && left.target.id == right.target.id
        });
        let mut seen = std::collections::HashSet::new();
        let mut items = Vec::new();
        for skill in discovered {
            let installation_id =
                InventoryItem::canonical_installation_id(&skill.identity.id, &skill.target.id);
            let installation = state
                .installations
                .iter()
                .find(|installation| installation.id == installation_id);
            let status = installation.map(|record| {
                if record.status == InstallationStatus::Failed {
                    InstallationStatus::Failed
                } else if record.content_digest.is_none() {
                    InstallationStatus::Unverified
                } else if record
                    .content_digest
                    .as_ref()
                    .is_some_and(|digest| digest != &skill.content_digest)
                {
                    InstallationStatus::Modified
                } else if matches!(
                    record.status,
                    InstallationStatus::Missing | InstallationStatus::Planned
                ) {
                    InstallationStatus::Installed
                } else {
                    record.status
                }
            });
            if let Some(record) = installation {
                seen.insert(record.id.as_str());
            }
            items.push(InventoryItem {
                action_id: installation_id.clone(),
                kind: "installation".to_string(),
                identity: skill.identity.clone(),
                target: Some(skill.target.clone()),
                installation_id,
                path: skill.path,
                scope: installation_scope(&skill.target.path),
                tracked: installation.is_some(),
                tracking_state: if installation.is_some() {
                    "tracked"
                } else {
                    "untracked"
                }
                .to_string(),
                watch_state: if watched_exactly(
                    &state.watchlists,
                    &skill.identity.source,
                    &skill.identity.relative_path,
                ) {
                    "watched"
                } else {
                    "not_watched"
                }
                .to_string(),
                lifecycle_state: Some(status.unwrap_or(InstallationStatus::Installed)),
                status,
                source_revision: installation
                    .and_then(|record| record.source_revision.clone())
                    .or(skill.source_revision),
                provenance: provenance(&skill.identity.source),
                description_excerpt: InventoryItem::description_excerpt(&skill.description),
                description: skill.description.clone(),
            });
        }
        for installation in &state.installations {
            if seen.contains(installation.id.as_str()) {
                continue;
            }
            let Some(identity) = state
                .identities
                .iter()
                .find(|identity| identity.id == installation.skill_id)
            else {
                continue;
            };
            let Some(target) = state
                .targets
                .iter()
                .find(|target| target.id == installation.target_id)
            else {
                continue;
            };
            let description = fs::read_to_string(Path::new(&installation.path).join("SKILL.md"))
                .ok()
                .and_then(|contents| frontmatter_value(&contents, "description"))
                .unwrap_or_default();
            items.push(InventoryItem {
                action_id: installation.id.clone(),
                kind: "installation".to_string(),
                identity: identity.clone(),
                target: Some(target.clone()),
                installation_id: installation.id.clone(),
                path: installation.path.clone(),
                scope: installation_scope(&target.path),
                tracked: true,
                tracking_state: "tracked".to_string(),
                watch_state: if watched_exactly(
                    &state.watchlists,
                    &identity.source,
                    &identity.relative_path,
                ) {
                    "watched"
                } else {
                    "not_watched"
                }
                .to_string(),
                lifecycle_state: Some(if installation.status == InstallationStatus::Failed {
                    InstallationStatus::Failed
                } else {
                    InstallationStatus::Missing
                }),
                status: Some(if installation.status == InstallationStatus::Failed {
                    InstallationStatus::Failed
                } else {
                    InstallationStatus::Missing
                }),
                source_revision: installation.source_revision.clone(),
                provenance: provenance(&identity.source),
                description_excerpt: InventoryItem::description_excerpt(&description),
                description,
            });
        }
        for watchlist in &state.watchlists {
            let watched_path = watchlist.path.as_deref();
            if watched_path.is_some_and(|path| {
                items.iter().any(|item| {
                    item.identity.source == watchlist.source.locator
                        && normalized_skill_path(&item.identity.relative_path)
                            == normalized_skill_path(path)
                        && item.lifecycle_state == Some(InstallationStatus::Installed)
                })
            }) {
                continue;
            }
            let path = watched_path.unwrap_or_default().to_string();
            let description = String::new();
            items.push(InventoryItem {
                action_id: watchlist.id.clone(),
                kind: "watchlist".to_string(),
                identity: SkillIdentity {
                    id: format!("watchlist:{}", watchlist.id),
                    name: watched_path
                        .and_then(|path| Path::new(path).file_name())
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_else(|| watchlist.source.locator.clone()),
                    source: watchlist.source.locator.clone(),
                    relative_path: path.clone(),
                },
                target: None,
                installation_id: String::new(),
                path,
                scope: "profile".to_string(),
                tracked: false,
                tracking_state: "not_applicable".to_string(),
                watch_state: "watched".to_string(),
                lifecycle_state: None,
                status: None,
                source_revision: watchlist
                    .last_observed_revision
                    .as_ref()
                    .map(|revision| revision.value.clone()),
                provenance: provenance(&watchlist.source.locator),
                description_excerpt: InventoryItem::description_excerpt(&description),
                description,
            });
        }
        items.sort_by(|left, right| {
            inventory_priority(left)
                .cmp(&inventory_priority(right))
                .then_with(|| {
                    left.identity
                        .name
                        .to_lowercase()
                        .cmp(&right.identity.name.to_lowercase())
                })
                .then_with(|| left.action_id.cmp(&right.action_id))
        });
        Ok(Inventory {
            schema_version: INVENTORY_SCHEMA_VERSION,
            profile: None,
            total: items.len(),
            offset: 0,
            limit: None,
            items,
        })
    }

    pub fn adopt(&self, skill_id: &str) -> Result<Installation, String> {
        let discovered = self
            .adapter
            .discover()?
            .into_iter()
            .find(|skill| skill.identity.id == skill_id)
            .ok_or_else(|| format!("no discovered skill has identity '{skill_id}'"))?;
        let installation_id = InventoryItem::canonical_installation_id(
            &discovered.identity.id,
            &discovered.target.id,
        );
        let mut state = self.repository.load()?;
        validate_state_version(&state)?;
        if let Some(existing) = state
            .installations
            .iter()
            .find(|installation| installation.id == installation_id)
        {
            return Ok(existing.clone());
        }
        if !state
            .identities
            .iter()
            .any(|identity| identity.id == discovered.identity.id)
        {
            state.identities.push(discovered.identity.clone());
        }
        if !state
            .targets
            .iter()
            .any(|target| target.id == discovered.target.id)
        {
            state.targets.push(discovered.target.clone());
        }
        let installation = Installation {
            id: installation_id,
            skill_id: discovered.identity.id,
            target_id: discovered.target.id,
            path: discovered.path,
            source_revision: discovered.source_revision,
            content_digest: Some(discovered.content_digest),
            status: InstallationStatus::Installed,
            last_error: None,
        };
        state.installations.push(installation.clone());
        self.repository.save(&state)?;
        Ok(installation)
    }

    pub fn status(&self) -> Result<StateStatus, String> {
        let state = self.repository.load()?;
        validate_state_version(&state)?;
        let inventory = self.inventory_for(&state)?;
        Ok(StateStatus {
            schema_version: LOCAL_STATE_VERSION,
            state_version: state.version,
            profile: self.repository.profile(),
            identity_count: state.identities.len(),
            target_count: state.targets.len(),
            installation_count: state.installations.len(),
            discovered_count: inventory
                .items
                .iter()
                .filter(|item| item.kind == "installation")
                .count(),
            untracked_count: inventory
                .items
                .iter()
                .filter(|item| item.kind == "installation" && !item.tracked)
                .count(),
        })
    }
}

fn validate_state_version(state: &LocalState) -> Result<(), String> {
    if state.version != LOCAL_STATE_VERSION {
        return Err(format!(
            "unsupported Local State version {}; expected {}",
            state.version, LOCAL_STATE_VERSION
        ));
    }
    Ok(())
}

fn installation_scope(target_path: &str) -> String {
    let path = Path::new(target_path);
    if std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .is_some_and(|home| path.starts_with(PathBuf::from(home).join(".copilot").join("skills")))
    {
        "user".to_string()
    } else if std::env::current_dir()
        .ok()
        .is_some_and(|current| path.starts_with(current.join(".github").join("skills")))
    {
        "project".to_string()
    } else {
        "unknown".to_string()
    }
}

fn watched_exactly(watchlists: &[tasc_core::Watchlist], source: &str, relative_path: &str) -> bool {
    watchlists.iter().any(|watchlist| {
        watchlist.source.locator == source
            && watchlist.path.as_deref().is_some_and(|path| {
                normalized_skill_path(path) == normalized_skill_path(relative_path)
            })
    })
}

fn normalized_skill_path(path: &str) -> &str {
    path.trim_end_matches("/SKILL.md")
}

fn provenance(source: &str) -> ProvenanceEvidence {
    if source.starts_with("https://github.com/") {
        ProvenanceEvidence {
            kind: ProvenanceKind::GithubRepository,
            confidence: ProvenanceConfidence::Probable,
            raw_evidence: Some(source.to_string()),
            explanation: "GitHub repository declared by the skill source metadata.".to_string(),
        }
    } else if source.starts_with("file:") {
        ProvenanceEvidence {
            kind: ProvenanceKind::LocalWorkspace,
            confidence: ProvenanceConfidence::Inferred,
            raw_evidence: Some(source.to_string()),
            explanation: "Local file source indicates a workspace installation.".to_string(),
        }
    } else {
        ProvenanceEvidence {
            kind: ProvenanceKind::Unknown,
            confidence: ProvenanceConfidence::Unavailable,
            raw_evidence: Some(source.to_string()),
            explanation: "No supported provenance evidence was available.".to_string(),
        }
    }
}

fn inventory_priority(item: &InventoryItem) -> u8 {
    match item.lifecycle_state {
        Some(InstallationStatus::Failed | InstallationStatus::Missing) => 0,
        Some(InstallationStatus::Modified | InstallationStatus::Outdated) => 1,
        Some(InstallationStatus::Unverified | InstallationStatus::Planned) => 2,
        Some(InstallationStatus::Installed) if !item.tracked => 3,
        Some(InstallationStatus::Installed) => 4,
        None if item.watch_state == "watched" => 5,
        None => 6,
    }
}

#[derive(Debug, Clone)]
pub struct CopilotAdapter {
    roots: Vec<PathBuf>,
}

impl CopilotAdapter {
    pub fn new(roots: Vec<PathBuf>) -> Self {
        Self { roots }
    }

    pub fn local() -> Self {
        let mut roots = Vec::new();
        if let Ok(current_dir) = std::env::current_dir() {
            roots.push(current_dir.join(".github").join("skills"));
        }
        if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
            roots.push(PathBuf::from(home).join(".copilot").join("skills"));
        }
        Self::new(roots)
    }

    pub fn target(scope: &str) -> Result<AdapterTarget, String> {
        let path = match scope {
            "project" => canonicalize_or_create(
                std::env::current_dir()
                    .map_err(|error| format!("cannot locate project target: {error}"))?
                    .join(".github")
                    .join("skills"),
            )
            .map_err(|error| format!("cannot resolve project target: {error}"))?,
            "user" => {
                let home = std::env::var_os("HOME")
                    .or_else(|| std::env::var_os("USERPROFILE"))
                    .ok_or_else(|| {
                        "cannot locate user target: home directory is not set".to_string()
                    })?;
                canonicalize_or_create(PathBuf::from(home).join(".copilot").join("skills"))
                    .map_err(|error| format!("cannot resolve user target: {error}"))?
            }
            _ => return Err(format!("unsupported Copilot installation scope '{scope}'")),
        };
        let source = local_source(&path);
        Ok(AdapterTarget {
            id: format!("target:copilot:{source}"),
            assistant: "copilot".to_string(),
            path: path.to_string_lossy().into_owned(),
        })
    }
}

fn canonicalize_normalized(path: PathBuf) -> std::io::Result<PathBuf> {
    // A missing target directory is a valid state when a fresh project/user scope is being
    // initialized; callers that need a path should call canonicalize_or_create instead.
    let canonical = fs::canonicalize(&path)?;
    Ok(normalize_private_root(canonical))
}

fn canonicalize_or_create(path: PathBuf) -> std::io::Result<PathBuf> {
    if !path.exists() {
        fs::create_dir_all(&path)?;
    }
    let canonical = fs::canonicalize(&path)?;
    Ok(normalize_private_root(canonical))
}

fn normalize_private_root(path: PathBuf) -> PathBuf {
    if cfg!(target_os = "macos") {
        let string = path.to_string_lossy();
        if let Some(rest) = string.strip_prefix("/private/") {
            return PathBuf::from(format!("/{rest}"));
        }
    }
    path
}

impl SkillAdapter for CopilotAdapter {
    fn discover(&self) -> Result<Vec<DiscoveredSkill>, String> {
        let mut skills = Vec::new();
        for root in &self.roots {
            let canonical_root = match canonicalize_normalized(root.clone()) {
                Ok(path) => path,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => {
                    return Err(format!(
                        "cannot resolve skill target '{}': {error}",
                        root.display()
                    ))
                }
            };
            let source = local_source(&canonical_root);
            let target = AdapterTarget {
                id: format!("target:copilot:{source}"),
                assistant: "copilot".to_string(),
                path: canonical_root.to_string_lossy().into_owned(),
            };
            let mut directories = Vec::new();
            collect_skill_directories(&canonical_root, &mut directories)?;
            for path in directories {
                let directory = fs::canonicalize(&path)
                    .map_err(|error| format!("cannot resolve '{}': {error}", path.display()))?;
                let relative_path = path
                    .strip_prefix(&canonical_root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                let contents = fs::read_to_string(path.join("SKILL.md"))
                    .map_err(|error| format!("cannot read '{}': {error}", path.display()))?;
                let content_digest = skill_content_digest(&path)?;
                let remote_source = frontmatter_value(&contents, "github-repo")
                    .filter(|value| value.starts_with("https://github.com/"));
                let source_locator = remote_source
                    .map(|source| {
                        source
                            .trim_end_matches('/')
                            .trim_end_matches(".git")
                            .to_ascii_lowercase()
                    })
                    .unwrap_or_else(|| source.clone());
                let remote_path = frontmatter_value(&contents, "github-path");
                let identity_path = remote_path.unwrap_or_else(|| relative_path.clone());
                let identity = SkillIdentity {
                    id: format!("skill:{source_locator}/{}", encode_uri_path(&identity_path)),
                    name: skill_name(&path.join("SKILL.md")).unwrap_or_else(|| {
                        path.file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned()
                    }),
                    source: source_locator,
                    relative_path: identity_path,
                };
                let source_revision = frontmatter_value(&contents, "github-tree-sha")
                    .or_else(|| frontmatter_value(&contents, "github-sha"))
                    .or_else(|| Some(format!("sha256:{content_digest}")));
                skills.push(DiscoveredSkill {
                    identity,
                    target: target.clone(),
                    path: directory.to_string_lossy().into_owned(),
                    description: frontmatter_value(&contents, "description").unwrap_or_default(),
                    source_revision,
                    content_digest,
                });
            }
        }

        skills.sort_by(|left, right| left.identity.id.cmp(&right.identity.id));
        Ok(skills)
    }
}

fn collect_skill_directories(root: &Path, skills: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = fs::read_dir(root)
        .map_err(|error| format!("cannot read skill target '{}': {error}", root.display()))?;
    for entry in entries {
        let entry = entry
            .map_err(|error| format!("cannot read skill entry in '{}': {error}", root.display()))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|error| format!("cannot inspect '{}': {error}", path.display()))?;
        if !file_type.is_dir() {
            continue;
        }
        if path.join(".tasc-draft.json").exists() {
            continue;
        }
        if path.join("SKILL.md").is_file() {
            skills.push(path);
        } else {
            collect_skill_directories(&path, skills)?;
        }
    }
    Ok(())
}

fn frontmatter_value(contents: &str, key: &str) -> Option<String> {
    let mut lines = contents.lines();
    if lines.next()?.trim() != "---" {
        return None;
    }
    let mut in_metadata = false;
    for line in lines {
        if line.trim() == "---" {
            break;
        }
        let trimmed = line.trim();
        if trimmed == "metadata:" {
            in_metadata = true;
            continue;
        }
        if let Some((name, value)) = trimmed.split_once(':') {
            let nested = line.starts_with(char::is_whitespace);
            if (key == name.trim() && !nested) || (in_metadata && key == name.trim() && nested) {
                let value = value.trim().trim_matches(['\'', '"']);
                if !value.is_empty() {
                    return Some(value.to_string());
                }
            }
        }
        if !line.starts_with(char::is_whitespace) && trimmed != "metadata:" {
            in_metadata = false;
        }
    }
    None
}

pub fn skill_content_digest(root: &Path) -> Result<String, String> {
    const MAX_SKILL_BYTES: u64 = 50 * 1024 * 1024;
    fn files(directory: &Path, output: &mut Vec<PathBuf>) -> Result<(), String> {
        for entry in fs::read_dir(directory)
            .map_err(|error| format!("cannot inspect skill '{}': {error}", directory.display()))?
        {
            let entry = entry.map_err(|error| format!("cannot inspect skill entry: {error}"))?;
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path)
                .map_err(|error| format!("cannot inspect '{}': {error}", path.display()))?;
            if metadata.file_type().is_symlink() || metadata.is_file() {
                output.push(path);
            } else if metadata.is_dir() {
                files(&path, output)?;
            }
        }
        Ok(())
    }

    let mut paths = Vec::new();
    files(root, &mut paths)?;
    paths.sort();
    let mut total = 0_u64;
    let mut digest = Sha256::new();
    for path in paths {
        let relative = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        digest.update(relative.as_bytes());
        digest.update([0]);
        let metadata = fs::symlink_metadata(&path)
            .map_err(|error| format!("cannot inspect '{}': {error}", path.display()))?;
        if metadata.file_type().is_symlink() {
            digest.update(
                fs::read_link(&path)
                    .map_err(|error| format!("cannot read link '{}': {error}", path.display()))?
                    .to_string_lossy()
                    .as_bytes(),
            );
        } else {
            total = total.saturating_add(metadata.len());
            if total > MAX_SKILL_BYTES {
                return Err("skill content exceeds the 50 MiB inspection limit".to_string());
            }
            digest.update(
                fs::read(&path)
                    .map_err(|error| format!("cannot read '{}': {error}", path.display()))?,
            );
        }
        digest.update([0]);
    }
    Ok(digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn local_source(path: &Path) -> String {
    let path = path.to_string_lossy().replace('\\', "/");
    let path = if path.starts_with('/') {
        path
    } else {
        format!("/{path}")
    };
    format!("file://{}", encode_uri_path(&path))
}

fn encode_uri_path(path: &str) -> String {
    let mut encoded = String::new();
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~/:".contains(&byte) {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

fn skill_name(path: &Path) -> Option<String> {
    let content = fs::read_to_string(path).ok()?;
    let mut lines = content.lines();
    if lines.next()?.trim() != "---" {
        return None;
    }
    for line in lines {
        if line.trim() == "---" {
            break;
        }
        if let Some((key, value)) = line.split_once(':') {
            if key.trim() == "name" {
                let name = value.trim().trim_matches(['\'', '"']);
                if !name.is_empty() {
                    return Some(name.to_string());
                }
            }
        }
    }
    None
}

#[derive(Debug, Clone)]
pub struct FsStateRepository {
    path: PathBuf,
}

impl FsStateRepository {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn default_path() -> Result<PathBuf, String> {
        if let Some(path) = std::env::var_os("TASC_STATE_PATH") {
            return Ok(PathBuf::from(path));
        }
        if let Some(state_home) = std::env::var_os("XDG_STATE_HOME") {
            return Ok(PathBuf::from(state_home).join("tasc").join("state.json"));
        }
        let home = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .ok_or_else(|| "cannot locate Local State: home directory is not set".to_string())?;
        Ok(PathBuf::from(home)
            .join(".local")
            .join("state")
            .join("tasc")
            .join("state.json"))
    }

    pub fn selected_profile_path() -> Result<(PathBuf, TascProfile), String> {
        if let Some(path) = std::env::var_os("TASC_STATE_PATH") {
            return Ok((
                PathBuf::from(path),
                TascProfile {
                    id: "custom".to_string(),
                    scope: ProfileScope::User,
                },
            ));
        }
        if let Ok(current_dir) = std::env::current_dir() {
            for directory in current_dir.ancestors() {
                let path = directory.join(".tasc").join("state.json");
                if path.is_file() {
                    return Ok((
                        path,
                        TascProfile {
                            id: "project".to_string(),
                            scope: ProfileScope::Project,
                        },
                    ));
                }
            }
        }
        Ok((
            Self::default_path()?,
            TascProfile {
                id: "user".to_string(),
                scope: ProfileScope::User,
            },
        ))
    }

    pub fn profile(&self) -> TascProfile {
        if std::env::var_os("TASC_STATE_PATH").is_some_and(|path| PathBuf::from(path) == self.path)
        {
            TascProfile {
                id: "custom".to_string(),
                scope: ProfileScope::User,
            }
        } else if self.path.parent().and_then(Path::file_name)
            == Some(std::ffi::OsStr::new(".tasc"))
        {
            TascProfile {
                id: "project".to_string(),
                scope: ProfileScope::Project,
            }
        } else {
            TascProfile {
                id: "user".to_string(),
                scope: ProfileScope::User,
            }
        }
    }

    pub fn load_read_only(&self) -> Result<LocalState, String> {
        self.load_with_migration(false)
    }

    fn load_with_migration(&self, migrate: bool) -> Result<LocalState, String> {
        let bytes = match fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(LocalState::default())
            }
            Err(error) => {
                return Err(format!(
                    "cannot read Local State '{}': {error}",
                    self.path.display()
                ))
            }
        };
        let state_version: serde_json::Value = serde_json::from_slice(&bytes)
            .map_err(|error| format!("invalid Local State '{}': {error}", self.path.display()))?;
        let raw_version = state_version
            .get("version")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| {
                format!(
                    "invalid Local State '{}': missing numeric version",
                    self.path.display()
                )
            })?;
        let version = u32::try_from(raw_version).map_err(|_| {
            format!("unsupported Local State version {raw_version}; expected {LOCAL_STATE_VERSION}")
        })?;
        let mut state: LocalState = serde_json::from_value(state_version)
            .map_err(|error| format!("invalid Local State '{}': {error}", self.path.display()))?;
        match version {
            1 => {
                state.version = LOCAL_STATE_VERSION;
                if migrate {
                    self.save(&state)?;
                }
            }
            LOCAL_STATE_VERSION => {}
            _ => {
                return Err(format!(
                    "unsupported Local State version {}; expected {}",
                    version, LOCAL_STATE_VERSION
                ))
            }
        }
        Ok(state)
    }
}

impl StateRepository for FsStateRepository {
    fn profile(&self) -> TascProfile {
        FsStateRepository::profile(self)
    }

    fn load(&self) -> Result<LocalState, String> {
        self.load_with_migration(true)
    }

    fn save(&self, state: &LocalState) -> Result<(), String> {
        if state.version != LOCAL_STATE_VERSION {
            return Err(format!(
                "unsupported Local State version {}; expected {}",
                state.version, LOCAL_STATE_VERSION
            ));
        }
        let parent = self
            .path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "cannot create Local State directory '{}': {error}",
                parent.display()
            )
        })?;
        let bytes = serde_json::to_vec_pretty(state)
            .map_err(|error| format!("cannot serialize Local State: {error}"))?;
        let (temp_path, mut file) = create_temp_file(parent, &self.path)?;
        if let Err(error) = file.write_all(&bytes).and_then(|_| file.sync_all()) {
            let _ = fs::remove_file(&temp_path);
            return Err(format!("cannot write Local State: {error}"));
        }
        drop(file);
        if let Err(error) = fs::rename(&temp_path, &self.path) {
            let _ = fs::remove_file(&temp_path);
            return Err(format!(
                "cannot atomically replace Local State '{}': {error}",
                self.path.display()
            ));
        }
        File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| format!("cannot sync Local State directory: {error}"))
    }
}

fn create_temp_file(parent: &Path, state_path: &Path) -> Result<(PathBuf, File), String> {
    for _ in 0..10 {
        let sequence = TEMP_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let name = format!(
            ".{}.{}.{}.tmp",
            state_path.file_name().unwrap_or_default().to_string_lossy(),
            std::process::id(),
            sequence
        );
        let path = parent.join(name);
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(format!("cannot create Local State temporary file: {error}")),
        }
    }
    Err("cannot allocate a Local State temporary file".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::{Arc, Mutex},
        time::{SystemTime, UNIX_EPOCH},
    };

    #[derive(Clone)]
    struct MemoryRepository(Arc<Mutex<LocalState>>);

    impl StateRepository for MemoryRepository {
        fn load(&self) -> Result<LocalState, String> {
            Ok(self.0.lock().unwrap().clone())
        }

        fn save(&self, state: &LocalState) -> Result<(), String> {
            *self.0.lock().unwrap() = state.clone();
            Ok(())
        }
    }

    struct MemoryAdapter(Vec<DiscoveredSkill>);

    impl SkillAdapter for MemoryAdapter {
        fn discover(&self) -> Result<Vec<DiscoveredSkill>, String> {
            Ok(self.0.clone())
        }
    }

    fn discovered_skill() -> DiscoveredSkill {
        let target = AdapterTarget {
            id: "target:copilot:/workspace/.github/skills".to_string(),
            assistant: "copilot".to_string(),
            path: "/workspace/.github/skills".to_string(),
        };
        DiscoveredSkill {
            identity: SkillIdentity {
                id: "skill:file:///workspace/.github/skills/review".to_string(),
                name: "review".to_string(),
                source: "file:///workspace/.github/skills".to_string(),
                relative_path: "review".to_string(),
            },
            target,
            path: "/workspace/.github/skills/review".to_string(),
            description: "Review code changes.".to_string(),
            source_revision: Some("tree-sha".to_string()),
            content_digest: "content-sha".to_string(),
        }
    }

    fn unique_temp_dir() -> PathBuf {
        let time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("tasc-test-{}-{time}", std::process::id()))
    }

    #[test]
    fn inventory_marks_untracked_and_adoption_only_updates_local_state() {
        let state = Arc::new(Mutex::new(LocalState::default()));
        let repository = MemoryRepository(state.clone());
        let skill = discovered_skill();
        let service = AgentService::new(MemoryAdapter(vec![skill.clone()]), repository.clone());

        let inventory = service.inventory().unwrap();
        assert_eq!(inventory.items.len(), 1);
        assert!(!inventory.items[0].tracked);

        let first = service.adopt(&skill.identity.id).unwrap();
        let second = service.adopt(&skill.identity.id).unwrap();
        assert_eq!(first, second);
        assert_eq!(state.lock().unwrap().identities, vec![skill.identity]);
        assert_eq!(state.lock().unwrap().targets, vec![skill.target]);
        assert_eq!(state.lock().unwrap().installations, vec![first]);
        assert!(service.inventory().unwrap().items[0].tracked);
    }

    #[test]
    fn inventory_projects_each_target_and_watched_only_interest_with_stable_ids() {
        let first = discovered_skill();
        let mut second = first.clone();
        second.target.id = "target:zed:/workspace/.config/zed/skills".to_string();
        second.target.assistant = "zed".to_string();
        second.target.path = "/workspace/.config/zed/skills".to_string();
        second.path = "/workspace/.config/zed/skills/review".to_string();
        let mut state = LocalState::default();
        state.watchlists.push(tasc_core::Watchlist {
            id: "watchlist:review-later".to_string(),
            source: tasc_core::discovery::SkillSource {
                id: "source:review".to_string(),
                locator: first.identity.source.clone(),
            },
            path: Some("future-skill".to_string()),
            last_observed_revision: None,
            last_check_id: None,
        });
        let service = AgentService::new(
            MemoryAdapter(vec![first.clone(), second.clone()]),
            MemoryRepository(Arc::new(Mutex::new(state))),
        );

        let inventory = service.inventory().unwrap();
        assert_eq!(inventory.total, 3);
        assert_eq!(
            inventory.items[0].action_id,
            inventory.items[0].installation_id
        );
        assert!(inventory.items.iter().any(|item| {
            item.action_id
                == InventoryItem::canonical_installation_id(&first.identity.id, &first.target.id)
                && item.lifecycle_state == Some(InstallationStatus::Installed)
                && item.tracking_state == "untracked"
                && item.watch_state == "not_watched"
                && item.provenance.confidence == ProvenanceConfidence::Inferred
        }));
        assert!(inventory.items.iter().any(|item| {
            item.target
                .as_ref()
                .is_some_and(|target| target.assistant == "zed")
        }));
        assert!(inventory
            .items
            .iter()
            .any(|item| item.action_id == "watchlist:review-later" && item.kind == "watchlist"));
        assert_eq!(service.inventory().unwrap().items, inventory.items);
    }

    #[test]
    fn filesystem_state_is_versioned_and_corruption_is_preserved_for_recovery() {
        let directory = unique_temp_dir();
        let path = directory.join("state.json");
        let repository = FsStateRepository::new(path.clone());
        repository.save(&LocalState::default()).unwrap();
        assert_eq!(repository.load().unwrap(), LocalState::default());
        assert!(
            serde_json::from_slice::<serde_json::Value>(&fs::read(&path).unwrap())
                .unwrap()
                .get("version")
                .is_some()
        );

        fs::write(&path, b"{broken").unwrap();
        let error = repository.load().unwrap_err();
        assert!(error.contains("invalid Local State"));
        assert_eq!(fs::read(&path).unwrap(), b"{broken");
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn version_one_installations_without_lifecycle_fields_remain_readable() {
        let state: LocalState = serde_json::from_str(
            r#"{"version":1,"identities":[{"id":"skill:old","name":"old","source":"file:///skills","relative_path":"old"}],"targets":[{"id":"target:old","assistant":"copilot","path":"/skills"}],"installations":[{"id":"installation:old","skill_id":"skill:old","target_id":"target:old","path":"/skills/old"}]}"#,
        )
        .unwrap();
        let installation = &state.installations[0];
        assert_eq!(installation.status, InstallationStatus::Installed);
        assert_eq!(installation.source_revision, None);
        assert_eq!(installation.content_digest, None);
        assert_eq!(installation.last_error, None);
    }

    #[test]
    fn loading_version_one_state_migrates_atomically_and_preserves_records() {
        let directory = unique_temp_dir();
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("state.json");
        let old = r#"{"version":1,"identities":[{"id":"skill:old","name":"old","source":"file:///skills","relative_path":"old"}],"targets":[{"id":"target:old","assistant":"copilot","path":"/skills"}],"installations":[{"id":"installation:old","skill_id":"skill:old","target_id":"target:old","path":"/skills/old"}]}"#;
        fs::write(&path, old).unwrap();

        let state = FsStateRepository::new(path.clone()).load().unwrap();
        assert_eq!(state.version, LOCAL_STATE_VERSION);
        assert_eq!(state.installations[0].id, "installation:old");
        let persisted: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(persisted["version"], LOCAL_STATE_VERSION);

        fs::write(&path, b"{broken").unwrap();
        assert!(FsStateRepository::new(path.clone()).load().is_err());
        assert_eq!(fs::read(&path).unwrap(), b"{broken");
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn read_only_state_load_does_not_migrate_version_one_files() {
        let directory = unique_temp_dir();
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("state.json");
        let old = r#"{"version":1,"identities":[],"targets":[],"installations":[]}"#;
        fs::write(&path, old).unwrap();

        let state = FsStateRepository::new(path.clone())
            .load_read_only()
            .unwrap();

        assert_eq!(state.version, LOCAL_STATE_VERSION);
        assert_eq!(fs::read_to_string(&path).unwrap(), old);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn copilot_adapter_discovers_skill_metadata_with_stable_ids() {
        let directory = unique_temp_dir();
        let root = directory.join(".github").join("skills");
        let skill_dir = root.join("review skill");
        fs::create_dir_all(&skill_dir).unwrap();
        let skill_file = skill_dir.join("SKILL.md");
        let contents = "---\nname: Code Review\ndescription: local only\n---\n";
        fs::write(&skill_file, contents).unwrap();
        let adapter = CopilotAdapter::new(vec![root]);

        let first = adapter.discover().unwrap();
        let second = adapter.discover().unwrap();
        assert_eq!(first, second);
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].identity.name, "Code Review");
        assert_eq!(first[0].identity.relative_path, "review skill");
        assert!(first[0].identity.id.ends_with("review%20skill"));
        assert_eq!(fs::read_to_string(&skill_file).unwrap(), contents);
        fs::remove_dir_all(directory).unwrap();
    }
}
