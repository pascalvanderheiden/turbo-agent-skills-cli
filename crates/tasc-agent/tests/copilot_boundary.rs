use serde_json::json;
use tasc_agent::conversation::{
    translate_sdk_event, translate_tool_call, AgentEvent, AgentPlan, AgentSession, AgentToolRequest,
};

#[test]
fn copilot_tool_calls_translate_only_to_typed_tasc_requests() {
    assert_eq!(
        translate_tool_call("tasc_search_skills", json!({"query": "review"})).unwrap(),
        AgentToolRequest::SearchSkills {
            query: "review".to_string()
        }
    );
    assert_eq!(
        translate_tool_call(
            "tasc_check_security",
            json!({"selectors": ["skill:one", "skill:two"], "allInstalled": false})
        )
        .unwrap(),
        AgentToolRequest::CheckSecurity {
            selectors: vec!["skill:one".to_string(), "skill:two".to_string()],
            all_installed: false
        }
    );
    assert_eq!(
        translate_tool_call("tasc_list_skills", json!({})).unwrap(),
        AgentToolRequest::ListSkills
    );
    assert_eq!(
        translate_tool_call("tasc_run_evaluation", json!({}))
            .unwrap_err()
            .code,
        "invalid_tool_arguments"
    );
    assert_eq!(
        translate_tool_call("bash", json!({})).unwrap_err().code,
        "unknown_agent_tool"
    );
}

#[test]
fn sdk_event_fixtures_translate_into_bounded_progress_events() {
    assert_eq!(
        translate_sdk_event(
            "tool.execution_start",
            &json!({"toolName": "tasc_check_security"})
        ),
        Some(AgentEvent::ToolStarted("tasc_check_security".to_string()))
    );
    assert_eq!(
        translate_sdk_event(
            "assistant.message_delta",
            &json!({"deltaContent": "Checking selected skills"})
        ),
        Some(AgentEvent::MessageDelta(
            "Checking selected skills".to_string()
        ))
    );
    assert_eq!(
        translate_sdk_event("assistant.turn_end", &json!({})),
        Some(AgentEvent::TurnFinished)
    );
    assert_eq!(translate_sdk_event("unrecognized.event", &json!({})), None);
}

#[test]
fn ranked_follow_ups_resolve_positions_groups_and_stale_references() {
    let mut session = AgentSession::default();
    session.remember_references(["skill:a", "skill:b", "skill:c"].map(str::to_string));

    assert_eq!(
        session.resolve_references("the second result").unwrap(),
        ["skill:b"]
    );
    assert_eq!(session.resolve_references("show #3").unwrap(), ["skill:c"]);
    assert_eq!(
        session.resolve_references("all three").unwrap(),
        ["skill:a", "skill:b", "skill:c"]
    );
    assert_eq!(session.resolve_references("skill:a").unwrap(), ["skill:a"]);
    let stale = session.resolve_references("the fourth one").unwrap_err();
    assert_eq!(stale.code, "ambiguous_agent_reference");
    assert_eq!(stale.candidates, ["skill:a", "skill:b", "skill:c"]);
}

#[test]
fn approval_tokens_bind_to_the_current_plan_and_are_single_use() {
    let plan = AgentPlan {
        operation: "install".to_string(),
        scope: "one skill".to_string(),
        target: "project".to_string(),
        revision: Some("abc123".to_string()),
        force: false,
        security_override_reason: None,
        state_fingerprint: "before".to_string(),
    };
    let mut session = AgentSession::default();
    let approval = session.prepare_approval(plan.clone());

    let mut changed = plan.clone();
    changed.state_fingerprint = "changed".to_string();
    assert_eq!(
        session
            .consume_approval(&approval.token, &changed)
            .unwrap_err()
            .code,
        "stale_approval"
    );
    assert_eq!(
        session
            .consume_approval(&approval.token, &plan)
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
