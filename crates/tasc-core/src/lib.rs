use serde::{Deserialize, Serialize};

pub mod discovery;

pub const LOCAL_STATE_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillIdentity {
    pub id: String,
    pub name: String,
    pub source: String,
    pub relative_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AdapterTarget {
    pub id: String,
    pub assistant: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Installation {
    pub id: String,
    pub skill_id: String,
    pub target_id: String,
    pub path: String,
    #[serde(default)]
    pub source_revision: Option<String>,
    #[serde(default)]
    pub content_digest: Option<String>,
    #[serde(default)]
    pub status: InstallationStatus,
    #[serde(default)]
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum InstallationStatus {
    Planned,
    #[default]
    Installed,
    Modified,
    Missing,
    Outdated,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalState {
    pub version: u32,
    pub identities: Vec<SkillIdentity>,
    pub targets: Vec<AdapterTarget>,
    pub installations: Vec<Installation>,
    #[serde(default)]
    pub sources: Vec<discovery::SkillSource>,
}

impl Default for LocalState {
    fn default() -> Self {
        Self {
            version: LOCAL_STATE_VERSION,
            identities: Vec::new(),
            targets: Vec::new(),
            installations: Vec::new(),
            sources: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InventoryItem {
    pub identity: SkillIdentity,
    pub target: AdapterTarget,
    pub installation_id: String,
    pub path: String,
    pub tracked: bool,
    #[serde(default)]
    pub status: Option<InstallationStatus>,
    #[serde(default)]
    pub source_revision: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Inventory {
    pub schema_version: u32,
    pub items: Vec<InventoryItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StateStatus {
    pub schema_version: u32,
    pub state_version: u32,
    pub identity_count: usize,
    pub target_count: usize,
    pub installation_count: usize,
    pub discovered_count: usize,
    pub untracked_count: usize,
}

impl InventoryItem {
    pub fn canonical_installation_id(skill_id: &str, target_id: &str) -> String {
        format!("installation:{skill_id}@{target_id}")
    }
}
