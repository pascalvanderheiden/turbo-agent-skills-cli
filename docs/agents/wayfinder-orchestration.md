# Wayfinder orchestration

Three GitHub Agentic Workflows automate the handoff after manual wayfinding:

1. `wayfinder-spec` finds a completed `wayfinder:map`, applies `to-spec`, posts
   the spec as a map comment, and adds `specced` plus `to-plan`.
2. `wayfinder-plan` applies `to-tickets` when `to-plan` is added, creates
   `ready-for-agent` implementation sub-issues with native dependency edges,
   then replaces `to-plan` with `planned`.
3. `wayfinder-delivery` reconciles planned maps every 30 minutes. It answers
   Copilot blocker questions, assigns one dependency-ready ticket at a time,
   merges eligible Copilot pull requests without a code-review step, and closes
   the map after all implementation work is complete.

The workflows keep agent permissions read-only. All GitHub mutations use gh-aw
safe outputs. Copilot assignment requires an Actions secret named
`GH_AW_AGENT_TOKEN` with permission to assign the Copilot coding agent. Repository
branch protection remains authoritative: the delivery workflow never bypasses
failed checks, pending checks, conflicts, or protected-branch requirements.

Compile changes with:

```bash
gh aw compile wayfinder-spec wayfinder-plan wayfinder-delivery
```
