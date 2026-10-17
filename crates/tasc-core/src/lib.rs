use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SkillSourceKind {
    GitHubRepository,
    LocalPath,
    RemoteArchive,
    Unknown,
}

impl std::fmt::Display for SkillSourceKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            SkillSourceKind::GitHubRepository => "github",
            SkillSourceKind::LocalPath => "local",
            SkillSourceKind::RemoteArchive => "archive",
            SkillSourceKind::Unknown => "unknown",
        };
        write!(f, "{label}")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillSource {
    pub id: String,
    pub name: String,
    pub kind: SkillSourceKind,
    pub url: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillRecord {
    pub id: String,
    pub name: String,
    pub source: String,
    pub version: String,
    pub path: Option<String>,
    pub installed: bool,
    pub assistant: String,
    pub summary: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WatchlistItem {
    pub id: String,
    pub source_id: String,
    pub label: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EvaluationResult {
    pub skill_id: String,
    pub scenario: String,
    pub score: f64,
    pub passed: bool,
    pub notes: Vec<String>,
    pub evaluated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConsistencyIssue {
    pub id: String,
    pub title: String,
    pub severity: String,
    pub description: String,
    pub implicated_skills: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Insight {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub priority: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TascState {
    pub sources: Vec<SkillSource>,
    pub watchlist: Vec<WatchlistItem>,
    pub skills: Vec<SkillRecord>,
}

impl Default for TascState {
    fn default() -> Self {
        Self {
            sources: vec![
                SkillSource {
                    id: "gh-skill-default".to_string(),
                    name: "GitHub Skills".to_string(),
                    kind: SkillSourceKind::GitHubRepository,
                    url: "https://github.com/skills".to_string(),
                    description: "Official GitHub-hosted skills and examples.".to_string(),
                },
                SkillSource {
                    id: "local-agent-skills".to_string(),
                    name: "Local agent skills".to_string(),
                    kind: SkillSourceKind::LocalPath,
                    url: "~/.copilot/skills".to_string(),
                    description: "Skills installed into the local coding assistant environment.".to_string(),
                },
            ],
            watchlist: vec![],
            skills: sample_skills(),
        }
    }
}

pub fn sample_skills() -> Vec<SkillRecord> {
    let now = Utc::now();
    vec![
        SkillRecord {
            id: "copilot-gh-cli".to_string(),
            name: "GitHub CLI skill".to_string(),
            source: "gh-skill-default".to_string(),
            version: "v1.0.0".to_string(),
            path: Some("~/.config/github-copilot/skills/gh-cli".to_string()),
            installed: true,
            assistant: "GitHub Copilot".to_string(),
            summary: "A shared wrapper around GitHub CLI workflows for repository and issue operations.".to_string(),
            updated_at: now,
        },
        SkillRecord {
            id: "tasc-core".to_string(),
            name: "TASC core skill".to_string(),
            source: "local-agent-skills".to_string(),
            version: "0.1.0".to_string(),
            path: Some("./.github/skills/tasc-core".to_string()),
            installed: false,
            assistant: "GitHub Copilot".to_string(),
            summary: "The shared knowledge and pattern model for TASC evaluation and consistency checks.".to_string(),
            updated_at: now,
        },
    ]
}

pub fn create_skill_source(name: &str, url: &str, kind: SkillSourceKind) -> SkillSource {
    SkillSource {
        id: format!("{}-{}", slugify(name), Uuid::new_v4().simple()),
        name: name.to_string(),
        kind,
        url: url.to_string(),
        description: format!("Auto-generated source for {}", name),
    }
}

pub fn slugify(value: &str) -> String {
    value
        .to_ascii_lowercase()
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '-')
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

pub fn default_insights() -> Vec<Insight> {
    vec![
        Insight {
            id: "insight-installation-health".to_string(),
            title: "Install health check".to_string(),
            summary: "Verify that installed skills still align with their source metadata and canonical identifiers.".to_string(),
            priority: "medium".to_string(),
        },
        Insight {
            id: "insight-eval-readiness".to_string(),
            title: "Evaluation readiness".to_string(),
            summary: "Before publishing any skill, run a consistent evaluation and confirm the main scenarios pass.".to_string(),
            priority: "high".to_string(),
        },
    ]
}

pub fn summarize_skill(skill: &SkillRecord) -> String {
    format!("{} [{}] ({})", skill.name, skill.version, skill.assistant)
}
