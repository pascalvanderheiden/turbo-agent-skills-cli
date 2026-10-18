use std::{
    collections::BTreeMap,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tasc_core::discovery::Diagnostic;

static PLAN_SEQUENCE: AtomicU64 = AtomicU64::new(0);
const APPROVAL_TTL: Duration = Duration::from_secs(5 * 60);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentToolRequest {
    ListSkills,
    SearchSkills {
        query: String,
    },
    ResolveReference {
        reference: String,
    },
    RunEvaluation {
        selector: String,
    },
    CheckConsistency {
        selectors: Vec<String>,
        all_installed: bool,
    },
    CheckSecurity {
        selectors: Vec<String>,
        all_installed: bool,
    },
    PlanLifecycle {
        operation: String,
        selector: String,
        force: bool,
        security_override_reason: Option<String>,
    },
    ListSecurity,
    ShowSecurity {
        id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentEvent {
    MessageDelta(String),
    ToolStarted(String),
    ToolCompleted(String),
    TurnFinished,
    Warning(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentPlan {
    pub operation: String,
    pub scope: String,
    pub target: String,
    pub revision: Option<String>,
    pub force: bool,
    pub security_override_reason: Option<String>,
    pub state_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalToken {
    pub token: String,
    pub plan: AgentPlan,
    pub expires_in_seconds: u64,
}

struct PendingApproval {
    plan: AgentPlan,
    expires_at: Instant,
}

#[derive(Default)]
pub struct AgentSession {
    references: Vec<String>,
    approvals: BTreeMap<String, PendingApproval>,
}

impl AgentSession {
    pub fn remember_references(&mut self, references: impl IntoIterator<Item = String>) {
        self.references = references.into_iter().collect();
    }

    pub fn resolve_references(&self, request: &str) -> Result<Vec<String>, Diagnostic> {
        let request = request.trim().to_ascii_lowercase();
        if let Some(id) = self
            .references
            .iter()
            .find(|id| id.to_ascii_lowercase() == request)
        {
            return Ok(vec![id.clone()]);
        }
        let tokens: Vec<_> = request
            .split(|character: char| !character.is_ascii_alphanumeric() && character != '#')
            .filter(|token| !token.is_empty())
            .collect();
        let all = tokens.iter().any(|token| matches!(*token, "all" | "both"));
        if all {
            let expected_count = tokens.iter().find_map(|token| token.parse::<usize>().ok());
            if tokens.contains(&"both") && self.references.len() != 2
                || expected_count.is_some_and(|count| count != self.references.len())
            {
                return Err(reference_error(&self.references));
            }
            return Ok(self.references.clone());
        }

        let position = tokens.iter().find_map(|token| ordinal(token)).or_else(|| {
            tokens
                .iter()
                .find_map(|token| token.strip_prefix('#')?.parse().ok())
        });
        if let Some(position) = position {
            if position == 0 || position > self.references.len() {
                return Err(reference_error(&self.references));
            }
            return Ok(vec![self.references[position - 1].clone()]);
        }

        Err(reference_error(&self.references))
    }

    pub fn prepare_approval(&mut self, plan: AgentPlan) -> ApprovalToken {
        let now = Instant::now();
        self.prepare_approval_at(plan, now)
    }

    pub fn consume_approval(
        &mut self,
        token: &str,
        current_plan: &AgentPlan,
    ) -> Result<AgentPlan, Diagnostic> {
        self.consume_approval_at(token, current_plan, Instant::now())
    }

    pub fn pending_plan(&self, token: &str) -> Option<AgentPlan> {
        self.approvals
            .get(token)
            .filter(|pending| pending.expires_at > Instant::now())
            .map(|pending| pending.plan.clone())
    }

    pub fn deny_approval(&mut self, token: &str) -> Result<(), Diagnostic> {
        self.approvals.remove(token).map(|_| ()).ok_or_else(|| {
            approval_error("stale_approval", "Approval token is unknown or expired.")
        })
    }

    fn prepare_approval_at(&mut self, plan: AgentPlan, now: Instant) -> ApprovalToken {
        self.approvals.retain(|_, pending| pending.expires_at > now);
        let sequence = PLAN_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let token = format!(
            "tasc-approve-{}",
            sha256(&format!(
                "{}:{}:{sequence}:{}",
                std::process::id(),
                nanos,
                plan.operation
            ))
        );
        self.approvals.insert(
            token.clone(),
            PendingApproval {
                plan: plan.clone(),
                expires_at: now + APPROVAL_TTL,
            },
        );
        ApprovalToken {
            token,
            plan,
            expires_in_seconds: APPROVAL_TTL.as_secs(),
        }
    }

    fn consume_approval_at(
        &mut self,
        token: &str,
        current_plan: &AgentPlan,
        now: Instant,
    ) -> Result<AgentPlan, Diagnostic> {
        let Some(pending) = self.approvals.remove(token) else {
            return Err(approval_error(
                "stale_approval",
                "Approval token is unknown, expired, or already used.",
            ));
        };
        if pending.expires_at <= now || pending.plan != *current_plan {
            return Err(approval_error(
                "stale_approval",
                "The approved action changed or its approval expired.",
            ));
        }
        Ok(pending.plan)
    }
}

pub fn translate_tool_call(
    name: &str,
    arguments: serde_json::Value,
) -> Result<AgentToolRequest, Diagnostic> {
    let invalid = || {
        Diagnostic::new(
            "invalid_tool_arguments",
            "usage",
            "copilot-sdk",
            "The requested TASC tool has invalid arguments.",
            "Use the declared TASC tool schema and exact selectors.",
        )
    };
    match name {
        "tasc_list_skills" => Ok(AgentToolRequest::ListSkills),
        "tasc_search_skills" => Ok(AgentToolRequest::SearchSkills {
            query: arguments
                .get("query")
                .and_then(serde_json::Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(invalid)?
                .to_string(),
        }),
        "tasc_resolve_reference" => Ok(AgentToolRequest::ResolveReference {
            reference: string_argument(&arguments, "reference").ok_or_else(invalid)?,
        }),
        "tasc_run_evaluation" => Ok(AgentToolRequest::RunEvaluation {
            selector: string_argument(&arguments, "selector").ok_or_else(invalid)?,
        }),
        "tasc_check_consistency" => Ok(AgentToolRequest::CheckConsistency {
            selectors: string_list(&arguments, "selectors").ok_or_else(invalid)?,
            all_installed: arguments
                .get("allInstalled")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
        }),
        "tasc_check_security" => Ok(AgentToolRequest::CheckSecurity {
            selectors: string_list(&arguments, "selectors").ok_or_else(invalid)?,
            all_installed: arguments
                .get("allInstalled")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
        }),
        "tasc_plan_lifecycle" => {
            let operation = string_argument(&arguments, "operation").ok_or_else(invalid)?;
            if !matches!(operation.as_str(), "update" | "remove") {
                return Err(invalid());
            }
            Ok(AgentToolRequest::PlanLifecycle {
                operation,
                selector: string_argument(&arguments, "selector").ok_or_else(invalid)?,
                force: arguments
                    .get("force")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false),
                security_override_reason: string_argument(&arguments, "securityOverrideReason"),
            })
        }
        "tasc_list_security_checks" => Ok(AgentToolRequest::ListSecurity),
        "tasc_show_security_check" => Ok(AgentToolRequest::ShowSecurity {
            id: string_argument(&arguments, "id").ok_or_else(invalid)?,
        }),
        _ => Err(Diagnostic::new(
            "unknown_agent_tool",
            "usage",
            "copilot-sdk",
            "The Copilot SDK requested an unknown TASC tool.",
            "Use only TASC tools registered for this session.",
        )),
    }
}

pub fn translate_sdk_event(name: &str, data: &serde_json::Value) -> Option<AgentEvent> {
    match name {
        "assistant.message_delta" => data
            .get("deltaContent")
            .and_then(serde_json::Value::as_str)
            .map(|text| AgentEvent::MessageDelta(text.to_string())),
        "tool.execution_start" => data
            .get("toolName")
            .and_then(serde_json::Value::as_str)
            .map(|tool| AgentEvent::ToolStarted(tool.to_string())),
        "tool.execution_complete" => data
            .get("toolName")
            .and_then(serde_json::Value::as_str)
            .map(|tool| AgentEvent::ToolCompleted(tool.to_string())),
        "assistant.turn_end" | "session.idle" => Some(AgentEvent::TurnFinished),
        "session.warning" | "session.error" => data
            .get("message")
            .and_then(serde_json::Value::as_str)
            .map(|message| AgentEvent::Warning(message.to_string())),
        _ => None,
    }
}

fn string_argument(arguments: &serde_json::Value, name: &str) -> Option<String> {
    arguments
        .get(name)
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToString::to_string)
}

fn string_list(arguments: &serde_json::Value, name: &str) -> Option<Vec<String>> {
    arguments
        .get(name)?
        .as_array()?
        .iter()
        .map(|value| value.as_str().map(ToString::to_string))
        .collect()
}

fn ordinal(token: &str) -> Option<usize> {
    match token {
        "first" | "1st" => Some(1),
        "second" | "2nd" => Some(2),
        "third" | "3rd" => Some(3),
        "fourth" | "4th" => Some(4),
        "fifth" | "5th" => Some(5),
        "sixth" | "6th" => Some(6),
        "seventh" | "7th" => Some(7),
        "eighth" | "8th" => Some(8),
        "ninth" | "9th" => Some(9),
        "tenth" | "10th" => Some(10),
        number => number.parse().ok(),
    }
}

fn reference_error(references: &[String]) -> Diagnostic {
    let mut error = Diagnostic::new(
        "ambiguous_agent_reference",
        "usage",
        "tasc-agent",
        "The conversational reference does not identify a current result.",
        "Use a current result position or exact identifier.",
    );
    error.candidates = references.to_vec();
    error
}

fn approval_error(code: &str, message: &str) -> Diagnostic {
    Diagnostic::new(
        code,
        "usage",
        "tasc-agent",
        message,
        "Prepare the action again and approve its current plan.",
    )
}

fn sha256(text: &str) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub fn fingerprint(parts: &[&[u8]]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update((part.len() as u64).to_be_bytes());
        hasher.update(part);
    }
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{AgentPlan, AgentSession, APPROVAL_TTL};
    use std::time::Instant;

    #[test]
    fn approval_tokens_expire_without_waiting() {
        let plan = AgentPlan {
            operation: "update".to_string(),
            scope: "skill:example".to_string(),
            target: "project".to_string(),
            revision: None,
            force: false,
            security_override_reason: None,
            state_fingerprint: "current".to_string(),
        };
        let mut session = AgentSession::default();
        let approval = session.prepare_approval(plan.clone());

        assert_eq!(
            session
                .consume_approval_at(&approval.token, &plan, Instant::now() + APPROVAL_TTL)
                .unwrap_err()
                .code,
            "stale_approval"
        );
    }

    #[test]
    fn approval_tokens_are_exact_plan_bound_and_single_use() {
        let plan = AgentPlan {
            operation: "update".to_string(),
            scope: "installation:example".to_string(),
            target: "project".to_string(),
            revision: Some("commit-a".to_string()),
            force: false,
            security_override_reason: None,
            state_fingerprint: "state-a".to_string(),
        };
        let mut changed = plan.clone();
        changed.revision = Some("commit-b".to_string());
        let mut session = AgentSession::default();
        let mismatch = session.prepare_approval(plan.clone());
        assert_eq!(
            session
                .consume_approval(&mismatch.token, &changed)
                .unwrap_err()
                .code,
            "stale_approval"
        );

        let approval = session.prepare_approval(plan.clone());
        assert_eq!(
            session.consume_approval(&approval.token, &plan).unwrap(),
            plan
        );
        assert_eq!(
            session
                .consume_approval(&approval.token, &plan)
                .unwrap_err()
                .code,
            "stale_approval"
        );
    }
}
