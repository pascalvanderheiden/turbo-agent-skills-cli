use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SkillDraft {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub purpose: String,
    pub workspace_path: String,
    pub skill_path: String,
    pub status: SkillDraftStatus,
    pub diagnostics: Vec<WorkspaceDiagnostic>,
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
    pub validated_at_unix_ms: u128,
}
