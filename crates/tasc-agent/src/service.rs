use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
use tasc_core::{
    AdapterTarget, Installation, Inventory, InventoryItem, LocalState, SkillIdentity, StateStatus,
    LOCAL_STATE_VERSION,
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
        let tracked = state
            .installations
            .iter()
            .map(|installation| installation.id.as_str())
            .collect::<std::collections::HashSet<_>>();
        let items = self
            .adapter
            .discover()?
            .into_iter()
            .map(|discovered| {
                let installation_id = InventoryItem::canonical_installation_id(
                    &discovered.identity.id,
                    &discovered.target.id,
                );
                InventoryItem {
                    identity: discovered.identity,
                    target: discovered.target,
                    path: discovered.path,
                    tracked: tracked.contains(installation_id.as_str()),
                    installation_id,
                }
            })
            .collect();
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
            let entries = fs::read_dir(&canonical_root).map_err(|error| {
                format!(
                    "cannot read skill target '{}': {error}",
                    canonical_root.display()
                )
            })?;
            let source = local_source(&canonical_root);
            let target = AdapterTarget {
                id: format!("target:copilot:{source}"),
                assistant: "copilot".to_string(),
                path: canonical_root.to_string_lossy().into_owned(),
            };
            for entry in entries {
                let entry = entry.map_err(|error| {
                    format!(
                        "cannot read skill entry in '{}': {error}",
                        canonical_root.display()
                    )
                })?;
                let path = entry.path();
                if !entry
                    .file_type()
                    .map_err(|error| format!("cannot inspect '{}': {error}", path.display()))?
                    .is_dir()
                    || !path.join("SKILL.md").is_file()
                {
                    continue;
                }
                let directory = fs::canonicalize(&path)
                    .map_err(|error| format!("cannot resolve '{}': {error}", path.display()))?;
                let relative_path = path
                    .strip_prefix(&canonical_root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                let identity = SkillIdentity {
                    id: format!(
                        "skill:{source}/{}",
                        encode_uri_path(&relative_path.replace('\\', "/"))
                    ),
                    name: skill_name(&path.join("SKILL.md"))
                        .unwrap_or_else(|| entry.file_name().to_string_lossy().into_owned()),
                    source: source.clone(),
                    relative_path,
                };
                skills.push(DiscoveredSkill {
                    identity,
                    target: target.clone(),
                    path: directory.to_string_lossy().into_owned(),
                });
            }
        }
        skills.sort_by(|left, right| left.identity.id.cmp(&right.identity.id));
        Ok(skills)
    }
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
