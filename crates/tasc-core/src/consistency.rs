use serde::{Deserialize, Serialize};

use crate::{SkillIdentity, SourceRevision};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConsistencyCategory {
    TriggerOverlap,
    TriggerGap,
    OwnershipOverlap,
    MissingDependency,
    IncompatibleAssumption,
    PrecedenceAmbiguity,
    TerminologyDrift,
    StaleArtifact,
    ScopeLeak,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ConsistencyStatus {
    #[serde(rename = "found")]
    Found,
    #[serde(rename = "not-found")]
    NotFound,
    #[serde(rename = "inconclusive")]
    Inconclusive,
    #[serde(rename = "not-run")]
    NotRun,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RevisionStatus {
    Current,
    Stale,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceConfidence {
    Observed,
    Declared,
    Inferred,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceLayer {
    Deterministic,
    Inferred,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FindingSeverity {
    Info,
    Warning,
    Blocking,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConsistencyEvidence {
    pub layer: EvidenceLayer,
    pub kind: String,
    pub skill_id: String,
    pub path: String,
    pub line: Option<usize>,
    pub excerpt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConsistencyFinding {
    pub id: String,
    pub category: ConsistencyCategory,
    pub status: ConsistencyStatus,
    pub revision_status: RevisionStatus,
    pub severity: FindingSeverity,
    pub confidence: EvidenceConfidence,
    pub confidence_score: u8,
    pub claim: String,
    pub skill_ids: Vec<String>,
    pub evidence: Vec<ConsistencyEvidence>,
    pub recommendation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConsistencyCategoryResult {
    pub category: ConsistencyCategory,
    pub status: ConsistencyStatus,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConsistencySkillSnapshot {
    pub identity: SkillIdentity,
    pub source_revision: Option<SourceRevision>,
    pub content_digest: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConsistencyArtifact {
    pub schema_version: u32,
    pub id: String,
    pub created_at_unix_ms: u128,
    pub completed_at_unix_ms: u128,
    pub selectors: Vec<String>,
    pub all_installed: bool,
    pub analyzer: String,
    pub analyzer_version: String,
    pub rule_version: String,
    pub skills: Vec<ConsistencySkillSnapshot>,
    pub categories: Vec<ConsistencyCategoryResult>,
    pub findings: Vec<ConsistencyFinding>,
    pub inferred_evidence_status: ConsistencyStatus,
}
