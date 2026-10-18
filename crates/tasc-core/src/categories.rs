use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SkillCategory {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
}

pub const SKILL_CATEGORIES: &[SkillCategory] = &[
    SkillCategory {
        id: "automation",
        name: "Automation",
        description: "Automates a bounded, repeatable task.",
    },
    SkillCategory {
        id: "coding",
        name: "Coding",
        description: "Supports software implementation and maintenance.",
    },
    SkillCategory {
        id: "documentation",
        name: "Documentation",
        description: "Creates, reviews, or maintains documentation.",
    },
    SkillCategory {
        id: "research",
        name: "Research",
        description: "Gathers and synthesizes information.",
    },
    SkillCategory {
        id: "security",
        name: "Security",
        description: "Supports security review and defensive practice.",
    },
    SkillCategory {
        id: "testing",
        name: "Testing",
        description: "Creates or evaluates tests and verification.",
    },
    SkillCategory {
        id: "workflow",
        name: "Workflow",
        description: "Guides a multi-step development workflow.",
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn category_descriptions_are_metadata_not_behavior() {
        assert_eq!(SKILL_CATEGORIES[0].id, "automation");
        assert!(!SKILL_CATEGORIES[0].description.is_empty());
    }
}
