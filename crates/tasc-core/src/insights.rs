use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InsightEvidenceSource {
    Lifecycle,
    Evaluation,
    Consistency,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InsightStatus {
    New,
    Acknowledged,
    Snoozed,
    Suppressed,
    Resolved,
    Stale,
    Inconclusive,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InsightPriority {
    pub blocking_impact: u8,
    pub confidence: u8,
    pub urgency: u8,
    pub affected_installations: usize,
    pub evidence_freshness: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InsightEvidence {
    pub source: InsightEvidenceSource,
    pub source_id: String,
    pub evidence_id: String,
    pub skill_ids: Vec<String>,
    pub revision: Option<String>,
    pub summary: String,
    pub severity: String,
    pub confidence: String,
    pub observed_at_unix_ms: Option<u128>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InsightSnapshot {
    pub schema_version: u32,
    pub id: String,
    pub insight_id: String,
    pub created_at_unix_ms: u128,
    pub title: String,
    pub recommendation: String,
    pub scope: Vec<String>,
    pub priority: InsightPriority,
    pub evidence: Vec<InsightEvidence>,
    pub evidence_version: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SuppressionScope {
    Insight,
    Evidence,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Suppression {
    pub id: String,
    pub insight_id: String,
    pub snapshot_id: String,
    pub scope: SuppressionScope,
    pub scope_id: String,
    pub evidence_version: String,
    pub reason: String,
    pub created_at_unix_ms: u128,
    pub expires_at_unix_ms: u128,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "action", content = "details")]
pub enum InsightLifecycleAction {
    Status {
        status: InsightStatus,
        reason: Option<String>,
        until_unix_ms: Option<u128>,
    },
    Suppressed(Suppression),
    Unsuppressed {
        suppression_id: String,
        reason: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InsightLifecycleEvent {
    pub id: String,
    pub insight_id: String,
    pub snapshot_id: String,
    pub evidence_version: String,
    pub created_at_unix_ms: u128,
    pub action: InsightLifecycleAction,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProposalRisk {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProposalReversibility {
    Reversible,
    ReviewRequired,
    NotApplicable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RemediationProposal {
    pub id: String,
    pub insight_id: String,
    pub snapshot_id: String,
    pub proposal_type: String,
    pub rationale: String,
    pub scope: Vec<String>,
    pub prerequisites: Vec<String>,
    pub affected_evidence: Vec<String>,
    pub risk: ProposalRisk,
    pub reversibility: ProposalReversibility,
    pub expected_verification: Vec<String>,
}
