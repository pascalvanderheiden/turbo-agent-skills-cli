use serde::{Deserialize, Serialize};

use crate::{AdapterTarget, SkillIdentity, SourceRevision};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EvaluationOutcome {
    Passed,
    Failed,
    Inconclusive,
    NotRun,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EvaluationDiagnostic {
    pub code: String,
    pub severity: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EvaluationAnalyzer {
    pub name: String,
    pub version: String,
    pub configuration: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EvaluationPolicy {
    pub mode: String,
    pub package_validation: String,
    pub scenario_execution: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RunArtifact {
    pub schema_version: u32,
    pub id: String,
    pub created_at_unix_ms: u128,
    pub completed_at_unix_ms: u128,
    pub selector: String,
    pub identity: SkillIdentity,
    pub source_revision: Option<SourceRevision>,
    pub target: Option<AdapterTarget>,
    pub installation_id: Option<String>,
    pub package_path: Option<String>,
    pub package_digest: Option<String>,
    pub analyzer: EvaluationAnalyzer,
    pub policy: EvaluationPolicy,
    pub outcome: EvaluationOutcome,
    pub package_outcome: EvaluationOutcome,
    pub execution_outcome: EvaluationOutcome,
    pub diagnostics: Vec<EvaluationDiagnostic>,
}
