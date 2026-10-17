use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SkillSource {
    pub id: String,
    pub locator: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub repo: String,
    pub skill_name: String,
    pub namespace: String,
    pub description: String,
    pub stars: u64,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RankedSkill {
    pub id: String,
    pub rank: usize,
    pub reason: String,
    pub skill: SearchHit,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub source: SkillSource,
    pub path: String,
    pub rendered_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    pub version: String,
    pub search: bool,
    pub preview: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    pub code: String,
    pub category: String,
    pub origin: Box<str>,
    pub message: Box<str>,
    pub remediation: Box<str>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub candidates: Vec<String>,
}

impl Diagnostic {
    pub fn new(code: &str, category: &str, origin: &str, message: &str, remediation: &str) -> Self {
        Self {
            code: code.into(),
            category: category.into(),
            origin: origin.into(),
            message: message.into(),
            remediation: remediation.into(),
            candidates: Vec::new(),
        }
    }

    pub fn exit_code(&self) -> i32 {
        match self.category.as_str() {
            "usage" => 2,
            "dependency" => 3,
            "finding" => 1,
            "cancelled" => 130,
            _ => 4,
        }
    }
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {} {}", self.code, self.message, self.remediation)?;
        if !self.candidates.is_empty() {
            write!(f, " Candidates: {}", self.candidates.join(", "))?;
        }
        Ok(())
    }
}

impl std::error::Error for Diagnostic {}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Envelope<T> {
    pub schema_version: u32,
    pub command: String,
    pub result: T,
    pub warnings: Vec<String>,
}

impl<T> Envelope<T> {
    pub fn new(command: &str, result: T) -> Self {
        Self {
            schema_version: 1,
            command: command.into(),
            result,
            warnings: Vec::new(),
        }
    }
}
