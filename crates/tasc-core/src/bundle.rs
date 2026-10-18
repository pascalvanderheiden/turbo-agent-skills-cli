use serde::{Deserialize, Serialize};

use crate::{categories::SKILL_CATEGORIES, SkillIdentity};

pub const BUNDLE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SkillBundleManifest {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    #[serde(default)]
    pub categories: Vec<String>,
    pub skills: Vec<SkillBundleEntry>,
    pub target_intent: Vec<BundleTargetIntent>,
    pub policy: BundlePolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SkillBundleEntry {
    pub identity: SkillIdentity,
    pub order: u32,
    #[serde(default)]
    pub dependencies: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BundleTargetIntent {
    pub assistant: String,
    pub scope: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BundlePolicy {
    pub install_requires_approval: bool,
    pub publish_requires_approval: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BundleValidation {
    pub schema_version: u32,
    pub bundle_id: String,
    pub valid: bool,
    pub diagnostics: Vec<String>,
}

impl SkillBundleManifest {
    pub fn validate(&self) -> BundleValidation {
        let mut diagnostics = Vec::new();
        if self.schema_version != BUNDLE_SCHEMA_VERSION {
            diagnostics.push(format!(
                "unsupported bundle schemaVersion {}; expected {}",
                self.schema_version, BUNDLE_SCHEMA_VERSION
            ));
        }
        if !valid_slug(&self.id) || !valid_text(&self.name, 128) {
            diagnostics.push(
                "bundle identity requires a lowercase slug ID and a name of 1-128 characters"
                    .to_string(),
            );
        }
        if !valid_version(&self.version) {
            diagnostics.push("bundle version must use numeric major.minor.patch form".to_string());
        }
        if !valid_text(&self.description, 1024) {
            diagnostics.push("bundle description must contain 1-1024 characters".to_string());
        }
        let categories = self
            .categories
            .iter()
            .map(String::as_str)
            .collect::<std::collections::HashSet<_>>();
        if categories.len() != self.categories.len()
            || self
                .categories
                .iter()
                .any(|category| !SKILL_CATEGORIES.iter().any(|known| known.id == category))
        {
            diagnostics
                .push("bundle categories must be unique descriptive category IDs".to_string());
        }
        if self.skills.is_empty() {
            diagnostics.push("bundle must contain at least one Skill Identity".to_string());
        }
        let mut identities = std::collections::HashMap::new();
        let mut orders = std::collections::HashSet::new();
        for (index, entry) in self.skills.iter().enumerate() {
            if entry.order != index as u32 {
                diagnostics.push("skills must be listed in ascending order from zero".to_string());
            }
            if !valid_identity(&entry.identity) {
                diagnostics.push(
                    "a Skill Identity has an invalid ID, source, or relative path".to_string(),
                );
            }
            if identities
                .insert(entry.identity.id.as_str(), entry.order)
                .is_some()
            {
                diagnostics.push("bundle contains a duplicate Skill Identity".to_string());
            }
            if !orders.insert(entry.order) {
                diagnostics.push(format!("duplicate skill order {}", entry.order));
            }
        }
        if orders.len() != self.skills.len()
            || orders.iter().copied().min() != Some(0)
            || orders.iter().copied().max() != Some(self.skills.len().saturating_sub(1) as u32)
        {
            diagnostics
                .push("skill order values must be unique and contiguous from zero".to_string());
        }
        for entry in &self.skills {
            let mut dependencies = std::collections::HashSet::new();
            for dependency in &entry.dependencies {
                if dependency == &entry.identity.id
                    || !dependencies.insert(dependency)
                    || !identities
                        .get(dependency.as_str())
                        .is_some_and(|order| *order < entry.order)
                {
                    diagnostics.push(
                        "a Skill Identity has an unknown, repeated, self, or out-of-order dependency"
                            .to_string(),
                    );
                }
            }
        }
        let mut targets = std::collections::HashSet::new();
        if self.target_intent.is_empty()
            || self.target_intent.iter().any(|target| {
                !valid_slug(&target.assistant)
                    || !matches!(target.scope.as_str(), "user" | "project")
                    || !targets.insert((&target.assistant, &target.scope))
            })
        {
            diagnostics.push(
                "targetIntent must contain unique assistant and user/project scope selections"
                    .to_string(),
            );
        }
        if !self.policy.install_requires_approval || !self.policy.publish_requires_approval {
            diagnostics.push(
                "bundle installation and publication must require explicit approval".to_string(),
            );
        }
        BundleValidation {
            schema_version: BUNDLE_SCHEMA_VERSION,
            bundle_id: if valid_slug(&self.id) {
                self.id.clone()
            } else {
                "invalid".to_string()
            },
            valid: diagnostics.is_empty(),
            diagnostics,
        }
    }
}

fn valid_identity(identity: &SkillIdentity) -> bool {
    let source = identity.source.as_str();
    let github_source = source.strip_prefix("https://").unwrap_or(source);
    let github_parts = github_source.split('/').collect::<Vec<_>>();
    let valid_source = ((source.starts_with("github.com/")
        || source.starts_with("https://github.com/"))
        && github_parts.len() == 3
        && github_parts[0] == "github.com"
        && valid_slug(github_parts[1])
        && valid_slug(github_parts[2])
        && !source.contains(['?', '#', '@', ' ']))
        || (source.starts_with("file:///") && !source.contains(['?', '#']));
    let path = identity.relative_path.as_str();
    let valid_path = !path.is_empty()
        && !path.starts_with('/')
        && !path.contains('\\')
        && path
            .split('/')
            .all(|component| !component.is_empty() && component != "." && component != "..");
    let expected_id = format!("skill:{source}/{}", encode_path(path));
    identity.id == expected_id && valid_source && valid_path && valid_text(&identity.name, 128)
}

fn valid_slug(value: &str) -> bool {
    let bytes = value.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 64
        && bytes[0].is_ascii_lowercase()
        && (bytes[bytes.len() - 1].is_ascii_lowercase() || bytes[bytes.len() - 1].is_ascii_digit())
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
        && !value.contains("--")
}

fn valid_version(version: &str) -> bool {
    let mut parts = version.split('.');
    let Some(major) = parts.next() else {
        return false;
    };
    let Some(minor) = parts.next() else {
        return false;
    };
    let Some(patch) = parts.next() else {
        return false;
    };
    [major, minor, patch]
        .into_iter()
        .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
        && parts.next().is_none()
}

fn encode_path(path: &str) -> String {
    let mut encoded = String::new();
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~/:".contains(&byte) {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

fn valid_text(value: &str, limit: usize) -> bool {
    let length = value.trim().chars().count();
    length > 0 && length <= limit
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest() -> SkillBundleManifest {
        SkillBundleManifest {
            schema_version: BUNDLE_SCHEMA_VERSION,
            id: "review-workflow".to_string(),
            name: "Review workflow".to_string(),
            version: "1.0.0".to_string(),
            description: "Skills for a review workflow.".to_string(),
            categories: vec!["coding".to_string()],
            skills: vec![
                SkillBundleEntry {
                    identity: SkillIdentity {
                        id: "skill:github.com/owner/repo/first".to_string(),
                        name: "First".to_string(),
                        source: "github.com/owner/repo".to_string(),
                        relative_path: "first".to_string(),
                    },
                    order: 0,
                    dependencies: Vec::new(),
                },
                SkillBundleEntry {
                    identity: SkillIdentity {
                        id: "skill:github.com/owner/repo/second".to_string(),
                        name: "Second".to_string(),
                        source: "github.com/owner/repo".to_string(),
                        relative_path: "second".to_string(),
                    },
                    order: 1,
                    dependencies: vec!["skill:github.com/owner/repo/first".to_string()],
                },
            ],
            target_intent: vec![BundleTargetIntent {
                assistant: "copilot".to_string(),
                scope: "project".to_string(),
            }],
            policy: BundlePolicy {
                install_requires_approval: true,
                publish_requires_approval: true,
            },
        }
    }

    #[test]
    fn bundle_manifest_validates_order_dependencies_target_and_approval_intent() {
        assert!(manifest().validate().valid);
    }

    #[test]
    fn bundle_manifest_rejects_invalid_identity_order_target_and_policy() {
        let mut manifest = manifest();
        manifest.skills[1].identity.id = "invalid".to_string();
        manifest.skills[1].order = 0;
        manifest.skills[1].dependencies = vec!["missing".to_string()];
        manifest.target_intent[0].scope = "installation".to_string();
        manifest.policy.install_requires_approval = false;

        let validation = manifest.validate();
        assert!(!validation.valid);
        assert!(validation.diagnostics.len() >= 4);
    }
}
