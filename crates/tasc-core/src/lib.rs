use serde::{Deserialize, Serialize};

pub mod consistency;
pub mod discovery;
pub mod evaluation;

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
    Unverified,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalState {
    pub version: u32,
    pub identities: Vec<SkillIdentity>,
    pub targets: Vec<AdapterTarget>,
    pub installations: Vec<Installation>,
    #[serde(default)]
    pub sources: Vec<discovery::SkillSource>,
    #[serde(default)]
    pub watchlists: Vec<Watchlist>,
}

impl Default for LocalState {
    fn default() -> Self {
        Self {
            version: LOCAL_STATE_VERSION,
            identities: Vec::new(),
            targets: Vec::new(),
            installations: Vec::new(),
            sources: Vec::new(),
            watchlists: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SourceRevision {
    pub kind: String,
    pub value: String,
}

impl SourceRevision {
    pub fn commit(value: String) -> Self {
        Self {
            kind: "commit".to_string(),
            value,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Watchlist {
    pub id: String,
    pub source: discovery::SkillSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    pub last_observed_revision: Option<SourceRevision>,
    pub last_check_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WatchCheckStatus {
    Baseline,
    Changed,
    Unchanged,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WatchCheck {
    pub id: String,
    pub watchlist_id: String,
    pub source: discovery::SkillSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    pub prior_revision: Option<SourceRevision>,
    pub observed_revision: Option<SourceRevision>,
    pub status: WatchCheckStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unavailable_reason: Option<String>,
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
