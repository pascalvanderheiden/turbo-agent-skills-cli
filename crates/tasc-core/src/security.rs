use serde::{Deserialize, Serialize};

use crate::{SkillIdentity, SourceRevision};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SecurityCategory {
    ExposedSecret,
    UnsafeInstruction,
    InjectionOrExfiltration,
    RiskyExecutableContent,
    SupplyChainIndicator,
    SuspiciousReference,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum SecuritySeverity {
    Info,
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SecurityEvidence {
    pub path: String,
    pub line: usize,
    pub excerpt: String,
    pub source_revision: Option<SourceRevision>,
    pub content_digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SecurityFinding {
    pub id: String,
    pub fingerprint: String,
    pub category: SecurityCategory,
    pub detector: String,
    pub severity: SecuritySeverity,
    pub confidence_score: u8,
    pub deterministic: bool,
    pub rationale: String,
    pub recommendation: String,
    pub skill_id: String,
    pub evidence: Vec<SecurityEvidence>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SecurityCheckStatus {
    Complete,
    Partial,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SecuritySkillSnapshot {
    pub identity: SkillIdentity,
    pub source_revision: Option<SourceRevision>,
    pub content_digest: Option<String>,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SecurityUnavailableCheck {
    pub name: String,
    pub status: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SecurityPolicy {
    pub fail_on: Option<SecuritySeverity>,
    pub minimum_confidence: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SecuritySuppression {
    pub id: String,
    pub fingerprint: String,
    pub reason: String,
    pub created_at_unix_ms: u128,
    pub expires_at_unix_ms: u128,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SecurityUnsuppression {
    pub id: String,
    pub suppression_id: String,
    pub reason: String,
    pub created_at_unix_ms: u128,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SecurityArtifact {
    pub schema_version: u32,
    pub id: String,
    pub created_at_unix_ms: u128,
    pub completed_at_unix_ms: u128,
    pub selectors: Vec<String>,
    pub all_installed: bool,
    pub analyzer: String,
    pub analyzer_version: String,
    pub rule_version: String,
    pub status: SecurityCheckStatus,
    pub policy: SecurityPolicy,
    pub skills: Vec<SecuritySkillSnapshot>,
    pub findings: Vec<SecurityFinding>,
    pub suppressed_fingerprints: Vec<String>,
    pub unavailable_checks: Vec<SecurityUnavailableCheck>,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SecurityRequirementStatus {
    Complete,
    Failed,
    Unavailable,
    Inconclusive,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SecurityRequirementEvidence {
    pub name: String,
    pub status: SecurityRequirementStatus,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LifecycleSecurityEvidence {
    pub policy: SecurityPolicy,
    pub source_revision: SourceRevision,
    pub content_digest: Option<String>,
    pub required_checks: Vec<SecurityRequirementEvidence>,
    pub findings: Vec<SecurityFinding>,
    pub diagnostics: Vec<String>,
}

impl LifecycleSecurityEvidence {
    pub fn required_checks_complete(&self) -> bool {
        self.required_checks
            .iter()
            .all(|check| check.status == SecurityRequirementStatus::Complete)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SecurityOverrideAudit {
    pub id: String,
    pub operation: String,
    pub plan_id: String,
    pub skill_id: String,
    pub target_id: String,
    pub source_revision: SourceRevision,
    pub reason: String,
    pub approved_at_unix_ms: u128,
}
