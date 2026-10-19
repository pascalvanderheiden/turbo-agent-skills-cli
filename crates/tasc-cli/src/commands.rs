#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandSafety {
    ReadOnly,
    PlanAndApproval,
}

#[derive(Clone, Copy, Debug)]
pub struct SlashCommand {
    pub name: &'static str,
    pub syntax: &'static str,
    pub aliases: &'static [&'static str],
    pub description: &'static str,
    pub arguments: &'static str,
    pub required_arguments: bool,
    pub safety: CommandSafety,
    pub active_allowed: bool,
}

pub const SLASH_COMMANDS: &[SlashCommand] = &[
    SlashCommand {
        name: "help",
        syntax: "help",
        aliases: &[],
        description: "Browse available Slash Commands.",
        arguments: "",
        required_arguments: false,
        safety: CommandSafety::ReadOnly,
        active_allowed: true,
    },
    SlashCommand {
        name: "status",
        syntax: "status",
        aliases: &[],
        description: "Show the current Agent Session and TASC Profile status.",
        arguments: "",
        required_arguments: false,
        safety: CommandSafety::ReadOnly,
        active_allowed: true,
    },
    SlashCommand {
        name: "cancel",
        syntax: "cancel",
        aliases: &[],
        description: "Cancel the active Agent Session operation.",
        arguments: "",
        required_arguments: false,
        safety: CommandSafety::ReadOnly,
        active_allowed: true,
    },
    SlashCommand {
        name: "quit",
        syntax: "quit",
        aliases: &["exit"],
        description: "End the Agent Session.",
        arguments: "",
        required_arguments: false,
        safety: CommandSafety::ReadOnly,
        active_allowed: false,
    },
    SlashCommand {
        name: "approve",
        syntax: "approve",
        aliases: &[],
        description: "Approve a pending lifecycle plan using its token.",
        arguments: "TOKEN",
        required_arguments: true,
        safety: CommandSafety::PlanAndApproval,
        active_allowed: false,
    },
    SlashCommand {
        name: "deny",
        syntax: "deny",
        aliases: &[],
        description: "Decline a pending lifecycle plan using its token.",
        arguments: "TOKEN",
        required_arguments: true,
        safety: CommandSafety::ReadOnly,
        active_allowed: false,
    },
    SlashCommand {
        name: "skill",
        syntax: "skill",
        aliases: &["skills"],
        description: "Browse or manage skills through authorized TASC operations.",
        arguments: "OPERATION [ARGUMENTS]",
        required_arguments: true,
        safety: CommandSafety::PlanAndApproval,
        active_allowed: false,
    },
    SlashCommand {
        name: "evaluation",
        syntax: "evaluation",
        aliases: &["eval", "evaluations"],
        description: "Run or inspect skill evaluations.",
        arguments: "OPERATION [ARGUMENTS]",
        required_arguments: true,
        safety: CommandSafety::ReadOnly,
        active_allowed: false,
    },
    SlashCommand {
        name: "consistency",
        syntax: "consistency",
        aliases: &["consistencies"],
        description: "Check or inspect skill consistency.",
        arguments: "OPERATION [ARGUMENTS]",
        required_arguments: true,
        safety: CommandSafety::ReadOnly,
        active_allowed: false,
    },
    SlashCommand {
        name: "security",
        syntax: "security",
        aliases: &["securities"],
        description: "Check or manage skill security findings.",
        arguments: "OPERATION [ARGUMENTS]",
        required_arguments: true,
        safety: CommandSafety::PlanAndApproval,
        active_allowed: false,
    },
    SlashCommand {
        name: "audit",
        syntax: "audit",
        aliases: &[],
        description: "Show the current audit report.",
        arguments: "[OPTIONS]",
        required_arguments: false,
        safety: CommandSafety::ReadOnly,
        active_allowed: false,
    },
    SlashCommand {
        name: "stats",
        syntax: "stats",
        aliases: &[],
        description: "Show skill statistics.",
        arguments: "[OPTIONS]",
        required_arguments: false,
        safety: CommandSafety::ReadOnly,
        active_allowed: false,
    },
    SlashCommand {
        name: "profile",
        syntax: "profile status",
        aliases: &["profiles"],
        description: "Show the active TASC Profile status.",
        arguments: "status",
        required_arguments: false,
        safety: CommandSafety::ReadOnly,
        active_allowed: false,
    },
    SlashCommand {
        name: "bundle",
        syntax: "bundle",
        aliases: &["bundles"],
        description: "Validate a skill bundle.",
        arguments: "validate [PATH]",
        required_arguments: true,
        safety: CommandSafety::ReadOnly,
        active_allowed: false,
    },
    SlashCommand {
        name: "command-skill",
        syntax: "command-skill",
        aliases: &["command-skills"],
        description: "List or manage Command Skills.",
        arguments: "OPERATION [ARGUMENTS]",
        required_arguments: true,
        safety: CommandSafety::PlanAndApproval,
        active_allowed: false,
    },
    SlashCommand {
        name: "model",
        syntax: "model",
        aliases: &[],
        description: "List models or select a model for this Agent Session.",
        arguments: "[list|MODEL|profile MODEL]",
        required_arguments: false,
        safety: CommandSafety::ReadOnly,
        active_allowed: false,
    },
];

pub fn canonical_command_input(input: &str) -> String {
    let input = input.trim();
    let Some(command) = input.strip_prefix('/').or_else(|| input.strip_prefix(':')) else {
        return input.to_string();
    };
    let (name, arguments) = command
        .split_once(char::is_whitespace)
        .map_or((command, ""), |(name, rest)| (name, rest));
    let canonical = SLASH_COMMANDS
        .iter()
        .find(|spec| {
            spec.name.eq_ignore_ascii_case(name)
                || spec
                    .aliases
                    .iter()
                    .any(|alias| alias.eq_ignore_ascii_case(name))
        })
        .map(|spec| spec.name)
        .unwrap_or(name);
    let mut normalized_arguments = arguments.trim_start().to_string();
    if name.eq_ignore_ascii_case("profiles") && normalized_arguments.is_empty() {
        normalized_arguments = "status".into();
    } else if name.eq_ignore_ascii_case("model")
        && normalized_arguments
            .split_once(char::is_whitespace)
            .is_some_and(|(subcommand, _)| subcommand.eq_ignore_ascii_case("save"))
    {
        normalized_arguments = normalized_arguments.replacen("save", "profile", 1);
    }
    if normalized_arguments.is_empty() {
        format!("/{canonical}")
    } else {
        format!("/{canonical} {normalized_arguments}")
    }
}

pub fn is_complete_command(input: &str) -> bool {
    let normalized = canonical_command_input(input);
    let Some(command) = normalized.strip_prefix('/') else {
        return false;
    };
    let (name, arguments) = command
        .split_once(char::is_whitespace)
        .map_or((command, ""), |(name, rest)| (name, rest.trim()));
    let Some(spec) = SLASH_COMMANDS.iter().find(|spec| spec.name == name) else {
        return false;
    };
    if spec.name == "profile" {
        return arguments == "status";
    }
    if spec.name == "bundle" {
        return arguments == "validate" || arguments.starts_with("validate ");
    }
    !spec.required_arguments || !arguments.is_empty()
}

pub fn ranked_commands(query: &str, active: bool) -> Vec<usize> {
    let query = query.trim().trim_start_matches(':').trim_start_matches('/');
    let query = query.split_whitespace().next().unwrap_or_default();
    let mut matches = SLASH_COMMANDS
        .iter()
        .enumerate()
        .filter_map(|(index, spec)| {
            let score = command_score(spec, query)?;
            Some((index, score))
        })
        .collect::<Vec<_>>();
    matches.sort_by_key(|(index, score)| {
        (
            usize::from(active && !SLASH_COMMANDS[*index].active_allowed),
            *score,
            *index,
        )
    });
    matches.into_iter().map(|(index, _)| index).collect()
}

pub fn alias_target(query: &str) -> Option<&'static str> {
    let query = query.trim().trim_start_matches(':').trim_start_matches('/');
    let name = query.split_whitespace().next()?;
    SLASH_COMMANDS
        .iter()
        .find(|spec| {
            spec.aliases
                .iter()
                .any(|alias| alias.eq_ignore_ascii_case(name))
        })
        .map(|spec| spec.name)
}

fn command_score(spec: &SlashCommand, query: &str) -> Option<usize> {
    if query.is_empty() {
        return Some(0);
    }
    let names = std::iter::once(spec.name).chain(spec.aliases.iter().copied());
    let mut best = None;
    for name in names {
        let name = name.to_ascii_lowercase();
        let query = query.to_ascii_lowercase();
        let score = if name == query {
            Some(0)
        } else if name.starts_with(&query) {
            Some(10 + name.len() - query.len())
        } else if name.contains(&query) {
            Some(20 + name.find(&query).unwrap_or_default())
        } else {
            subsequence_score(&query, &name).map(|score| 30 + score)
        };
        if let Some(score) = score {
            best = Some(best.map_or(score, |current: usize| current.min(score)));
        }
    }
    if let Some(best) = best {
        return Some(best);
    }
    let description = spec.description.to_ascii_lowercase();
    let query = query.to_ascii_lowercase();
    description
        .contains(&query)
        .then_some(60)
        .or_else(|| subsequence_score(&query, &description).map(|score| 70 + score))
}

fn subsequence_score(query: &str, text: &str) -> Option<usize> {
    let mut chars = text.char_indices();
    let mut previous = None;
    let mut gaps = 0;
    for target in query.chars() {
        let (position, _) = chars.find(|(_, character)| *character == target)?;
        if let Some(previous) = previous {
            gaps += position.saturating_sub(previous + 1);
        }
        previous = Some(position);
    }
    Some(gaps)
}

#[cfg(test)]
mod tests {
    use super::{alias_target, canonical_command_input, ranked_commands, SLASH_COMMANDS};

    #[test]
    fn aliases_normalize_to_canonical_commands_without_duplicate_results() {
        assert_eq!(
            canonical_command_input(":eval run example"),
            "/evaluation run example"
        );
        assert_eq!(
            canonical_command_input("/command-skills list"),
            "/command-skill list"
        );
        assert_eq!(canonical_command_input("/profiles"), "/profile status");
        assert_eq!(alias_target(":/eval"), Some("evaluation"));
        let results = ranked_commands("/eval", false);
        assert_eq!(results[0], 7);
        assert_eq!(results.iter().filter(|index| **index == 7).count(), 1);
    }

    #[test]
    fn ranking_prefers_exact_then_prefix_then_fuzzy_and_matches_descriptions() {
        let names = |query: &str| {
            ranked_commands(query, false)
                .into_iter()
                .map(|index| SLASH_COMMANDS[index].name)
                .collect::<Vec<_>>()
        };
        assert_eq!(names("eval")[0], "evaluation");
        assert_eq!(names("evalu")[0], "evaluation");
        assert!(names("evluation").contains(&"evaluation"));
        assert!(names("models").contains(&"model"));
        assert!(ranked_commands("no-such-command", false).is_empty());
    }

    #[test]
    fn active_operations_keep_unsafe_commands_visible_but_not_enabled() {
        let results = ranked_commands("", true);
        assert!(results.len() == SLASH_COMMANDS.len());
        assert!(results
            .iter()
            .any(|index| SLASH_COMMANDS[*index].name == "cancel"));
        assert!(results
            .iter()
            .any(|index| SLASH_COMMANDS[*index].name == "skill"));
    }
}
