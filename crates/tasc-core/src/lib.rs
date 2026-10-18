use serde::{Deserialize, Serialize};

pub mod authoring;
pub mod consistency;
pub mod discovery;
pub mod evaluation;
pub mod insights;
pub mod security;

pub const LOCAL_STATE_VERSION: u32 = 2;
pub const INVENTORY_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProfileScope {
    Project,
    User,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TascProfile {
    pub id: String,
    pub scope: ProfileScope,
}

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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
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
    pub action_id: String,
    #[serde(default = "default_inventory_kind")]
    pub kind: String,
    pub identity: SkillIdentity,
    #[serde(default)]
    pub target: Option<AdapterTarget>,
    #[serde(default)]
    pub installation_id: String,
    pub path: String,
    #[serde(default = "default_scope")]
    pub scope: String,
    pub tracked: bool,
    #[serde(default = "default_tracking_state")]
    pub tracking_state: String,
    #[serde(default = "default_watch_state")]
    pub watch_state: String,
    #[serde(default)]
    pub lifecycle_state: Option<InstallationStatus>,
    #[serde(default)]
    pub status: Option<InstallationStatus>,
    #[serde(default)]
    pub source_revision: Option<String>,
    #[serde(default)]
    pub provenance: ProvenanceEvidence,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub description_excerpt: String,
}

fn default_inventory_kind() -> String {
    "installation".to_string()
}

fn default_scope() -> String {
    "unknown".to_string()
}

fn default_tracking_state() -> String {
    "untracked".to_string()
}

fn default_watch_state() -> String {
    "not_watched".to_string()
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProvenanceKind {
    GithubRepository,
    MarketplacePlugin,
    LocalWorkspace,
    BundledFirstParty,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProvenanceConfidence {
    Verified,
    Probable,
    Inferred,
    #[default]
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProvenanceEvidence {
    pub kind: ProvenanceKind,
    pub confidence: ProvenanceConfidence,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_evidence: Option<String>,
    #[serde(default)]
    pub explanation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Inventory {
    pub schema_version: u32,
    #[serde(default)]
    pub profile: Option<TascProfile>,
    #[serde(default)]
    pub total: usize,
    #[serde(default)]
    pub offset: usize,
    #[serde(default)]
    pub limit: Option<usize>,
    pub items: Vec<InventoryItem>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InventoryQuery {
    pub status: Option<String>,
    pub scope: Option<String>,
    pub assistant: Option<String>,
    pub provenance: Option<String>,
    pub tracking: Option<String>,
    pub watch: Option<String>,
    pub sort: Option<String>,
    pub offset: usize,
    pub limit: Option<usize>,
    pub detail: Option<String>,
}

impl Inventory {
    pub fn select(&self, query: &InventoryQuery) -> Result<Self, String> {
        let mut items = self
            .items
            .iter()
            .filter(|item| {
                query.detail.as_deref().map_or(true, |detail| {
                    item.action_id == detail
                        || item.identity.id == detail
                        || item.installation_id == detail
                })
            })
            .filter(|item| {
                matches_filter(
                    Some(lifecycle_filter(item.lifecycle_state)),
                    query.status.as_deref(),
                )
            })
            .filter(|item| matches_filter(Some(item.scope.as_str()), query.scope.as_deref()))
            .filter(|item| {
                matches_filter(
                    item.target.as_ref().map(|target| target.assistant.as_str()),
                    query.assistant.as_deref(),
                )
            })
            .filter(|item| {
                matches_filter(
                    Some(provenance_filter(item.provenance.kind)),
                    query.provenance.as_deref(),
                )
            })
            .filter(|item| {
                matches_filter(
                    Some(item.tracking_state.as_str()),
                    query.tracking.as_deref(),
                )
            })
            .filter(|item| matches_filter(Some(item.watch_state.as_str()), query.watch.as_deref()))
            .cloned()
            .collect::<Vec<_>>();
        match query.sort.as_deref().unwrap_or("default") {
            "default" => {}
            "name" => items.sort_by(|left, right| {
                left.identity
                    .name
                    .to_lowercase()
                    .cmp(&right.identity.name.to_lowercase())
                    .then_with(|| left.action_id.cmp(&right.action_id))
            }),
            "lifecycle" => items.sort_by(|left, right| {
                lifecycle_filter(left.lifecycle_state)
                    .cmp(&lifecycle_filter(right.lifecycle_state))
                    .then_with(|| left.action_id.cmp(&right.action_id))
            }),
            "scope" => items.sort_by(|left, right| {
                left.scope
                    .cmp(&right.scope)
                    .then_with(|| left.action_id.cmp(&right.action_id))
            }),
            "assistant" => items.sort_by(|left, right| {
                left.target
                    .as_ref()
                    .map(|target| &target.assistant)
                    .cmp(&right.target.as_ref().map(|target| &target.assistant))
                    .then_with(|| left.action_id.cmp(&right.action_id))
            }),
            "provenance" => items.sort_by(|left, right| {
                provenance_filter(left.provenance.kind)
                    .cmp(provenance_filter(right.provenance.kind))
                    .then_with(|| left.action_id.cmp(&right.action_id))
            }),
            sort => return Err(format!("unsupported inventory sort '{sort}'")),
        }
        let total = items.len();
        if let Some(detail) = query.detail.as_deref() {
            if total == 0 {
                return Err(format!("no inventory row has stable ID '{detail}'"));
            }
        }
        let offset = query.offset.min(total);
        let limit = query.limit.unwrap_or(total.saturating_sub(offset));
        let items = items.into_iter().skip(offset).take(limit).collect();
        Ok(Self {
            schema_version: self.schema_version,
            profile: self.profile.clone(),
            total,
            offset,
            limit: query.limit,
            items,
        })
    }
}

fn matches_filter(value: Option<&str>, expected: Option<&str>) -> bool {
    expected.map_or(true, |expected| {
        value.is_some_and(|value| value.eq_ignore_ascii_case(expected))
    })
}

fn lifecycle_filter(status: Option<InstallationStatus>) -> &'static str {
    match status {
        Some(InstallationStatus::Planned) => "planned",
        Some(InstallationStatus::Installed) => "installed",
        Some(InstallationStatus::Modified) => "modified",
        Some(InstallationStatus::Missing) => "missing",
        Some(InstallationStatus::Outdated) => "outdated",
        Some(InstallationStatus::Failed) => "failed",
        Some(InstallationStatus::Unverified) => "unverified",
        None => "none",
    }
}

fn provenance_filter(kind: ProvenanceKind) -> &'static str {
    match kind {
        ProvenanceKind::GithubRepository => "github_repository",
        ProvenanceKind::MarketplacePlugin => "marketplace_plugin",
        ProvenanceKind::LocalWorkspace => "local_workspace",
        ProvenanceKind::BundledFirstParty => "bundled_first_party",
        ProvenanceKind::Unknown => "unknown",
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StateStatus {
    pub schema_version: u32,
    pub state_version: u32,
    pub profile: TascProfile,
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

    pub fn description_excerpt(description: &str) -> String {
        let normalized = description.split_whitespace().collect::<Vec<_>>().join(" ");
        let mut excerpt = normalized.chars().take(120).collect::<String>();
        if normalized.chars().count() > 120 {
            excerpt = normalized.chars().take(119).collect();
            excerpt.push('…');
        }
        excerpt
    }
}

#[cfg(test)]
mod inventory_tests {
    use super::*;

    fn item(action_id: &str, name: &str, scope: &str) -> InventoryItem {
        InventoryItem {
            action_id: action_id.to_string(),
            kind: "installation".to_string(),
            identity: SkillIdentity {
                id: format!("skill:{name}"),
                name: name.to_string(),
                source: "file:///skills".to_string(),
                relative_path: name.to_string(),
            },
            target: Some(AdapterTarget {
                id: format!("target:{scope}"),
                assistant: "copilot".to_string(),
                path: "/skills".to_string(),
            }),
            installation_id: action_id.to_string(),
            path: format!("/skills/{name}"),
            scope: scope.to_string(),
            tracked: false,
            tracking_state: "untracked".to_string(),
            watch_state: "not_watched".to_string(),
            lifecycle_state: Some(InstallationStatus::Installed),
            status: None,
            source_revision: None,
            provenance: ProvenanceEvidence::default(),
            description: "A test skill.".to_string(),
            description_excerpt: "A test skill.".to_string(),
        }
    }

    #[test]
    fn inventory_query_filters_sorts_pages_and_selects_by_stable_id() {
        let inventory = Inventory {
            schema_version: INVENTORY_SCHEMA_VERSION,
            profile: None,
            total: 2,
            offset: 0,
            limit: None,
            items: vec![
                item("installation:z@target:user", "Zebra", "user"),
                item("installation:a@target:project", "Alpha", "project"),
            ],
        };
        let page = inventory
            .select(&InventoryQuery {
                scope: Some("PROJECT".to_string()),
                sort: Some("name".to_string()),
                offset: 0,
                limit: Some(1),
                ..InventoryQuery::default()
            })
            .unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.items[0].action_id, "installation:a@target:project");
        let selected = inventory
            .select(&InventoryQuery {
                detail: Some("installation:z@target:user".to_string()),
                ..InventoryQuery::default()
            })
            .unwrap();
        assert_eq!(selected.items[0].identity.name, "Zebra");
        assert!(inventory
            .select(&InventoryQuery {
                detail: Some("row-1".to_string()),
                ..InventoryQuery::default()
            })
            .is_err());
    }

    #[test]
    fn description_excerpt_is_normalized_and_bounded() {
        let excerpt = InventoryItem::description_excerpt(&format!("  {}  ", "é".repeat(121)));
        assert_eq!(excerpt.chars().count(), 120);
        assert!(excerpt.ends_with('…'));
    }
}
