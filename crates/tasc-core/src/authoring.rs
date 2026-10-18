use serde::{Deserialize, Serialize};

pub const SKILL_DRAFT_SCHEMA_VERSION: u32 = 2;
pub const WORKSPACE_VALIDATION_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SkillDraftStatus {
    Incomplete,
    Ready,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LocalContext {
    pub path: String,
    pub digest: String,
    pub headings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceDiagnostic {
    pub code: String,
    pub severity: String,
    pub path: Option<String>,
    pub message: String,
    pub remediation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct SkillDraftAnswers {
    #[serde(default)]
    pub purpose: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub triggers: String,
    #[serde(default)]
    pub non_goals: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SkillDraft {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub purpose: String,
    pub workspace_path: String,
    pub skill_path: String,
    #[serde(default)]
    pub answers: SkillDraftAnswers,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub categories: Vec<String>,
    pub status: SkillDraftStatus,
    pub diagnostics: Vec<WorkspaceDiagnostic>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub quality_guidance: Vec<WorkspaceDiagnostic>,
    pub local_context: Option<LocalContext>,
    pub recovery_path: Option<String>,
    pub created_at_unix_ms: u128,
    pub updated_at_unix_ms: u128,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceValidation {
    pub schema_version: u32,
    pub draft_id: String,
    pub status: SkillDraftStatus,
    pub diagnostics: Vec<WorkspaceDiagnostic>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub quality_guidance: Vec<WorkspaceDiagnostic>,
    pub validated_at_unix_ms: u128,
}
