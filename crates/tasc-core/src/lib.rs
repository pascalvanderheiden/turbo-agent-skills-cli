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
