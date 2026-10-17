use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
use tasc_core::{
    AdapterTarget, Installation, InstallationStatus, Inventory, InventoryItem, LocalState,
    SkillIdentity, StateStatus, LOCAL_STATE_VERSION,
};

static TEMP_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub trait SkillAdapter {
    fn discover(&self) -> Result<Vec<DiscoveredSkill>, String>;
}

pub trait StateRepository {
    fn load(&self) -> Result<LocalState, String>;
    fn save(&self, state: &LocalState) -> Result<(), String>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredSkill {
    pub identity: SkillIdentity,
    pub target: AdapterTarget,
    pub path: String,
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
        self.inventory_for(&state)
    }

    fn inventory_for(&self, state: &LocalState) -> Result<Inventory, String> {
        let discovered = self.adapter.discover()?;
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
                } else {
                    record.status
                }
            });
            if let Some(record) = installation {
                seen.insert(record.id.as_str());
            }
            items.push(InventoryItem {
                identity: skill.identity,
                target: skill.target,
                installation_id,
                path: skill.path,
                tracked: installation.is_some(),
                status,
                source_revision: installation
                    .and_then(|record| record.source_revision.clone())
                    .or(skill.source_revision),
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
            items.push(InventoryItem {
                identity: identity.clone(),
                target: target.clone(),
                installation_id: installation.id.clone(),
                path: installation.path.clone(),
                tracked: true,
                status: Some(if installation.status == InstallationStatus::Failed {
                    InstallationStatus::Failed
                } else {
                    InstallationStatus::Missing
                }),
                source_revision: installation.source_revision.clone(),
            });
        }
        Ok(Inventory {
            schema_version: LOCAL_STATE_VERSION,
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
            identity_count: state.identities.len(),
            target_count: state.targets.len(),
            installation_count: state.installations.len(),
            discovered_count: inventory.items.len(),
            untracked_count: inventory.items.iter().filter(|item| !item.tracked).count(),
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
            "project" => fs::canonicalize(
                std::env::current_dir()
                    .map_err(|error| format!("cannot locate project target: {error}"))?,
            )
            .map_err(|error| format!("cannot resolve project target: {error}"))?
            .join(".github")
            .join("skills"),
            "user" => {
                let home = std::env::var_os("HOME")
                    .or_else(|| std::env::var_os("USERPROFILE"))
                    .ok_or_else(|| {
                        "cannot locate user target: home directory is not set".to_string()
                    })?;
                fs::canonicalize(PathBuf::from(home))
                    .map_err(|error| format!("cannot resolve user target: {error}"))?
                    .join(".copilot")
                    .join("skills")
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

impl SkillAdapter for CopilotAdapter {
    fn discover(&self) -> Result<Vec<DiscoveredSkill>, String> {
        let mut skills = Vec::new();
        for root in &self.roots {
            let canonical_root = match fs::canonicalize(root) {
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

pub(crate) fn skill_content_digest(root: &Path) -> Result<String, String> {
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
}

impl StateRepository for FsStateRepository {
    fn load(&self) -> Result<LocalState, String> {
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
        let state: LocalState = serde_json::from_slice(&bytes)
            .map_err(|error| format!("invalid Local State '{}': {error}", self.path.display()))?;
        if state.version != LOCAL_STATE_VERSION {
            return Err(format!(
                "unsupported Local State version {}; expected {}",
                state.version, LOCAL_STATE_VERSION
            ));
        }
        Ok(state)
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
