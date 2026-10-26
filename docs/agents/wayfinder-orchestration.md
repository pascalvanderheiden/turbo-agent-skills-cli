# Wayfinder orchestration

Four GitHub Agentic Workflows automate issue delivery:

1. `wayfinder-to-spec` adds `to-spec` to an open `wayfinder:map` when all native
   decision sub-issues are closed. Ordinary issues can be queued explicitly.
2. `wayfinder-spec` applies the `to-spec` skill, posts the spec as a comment on
   the same issue/map, then replaces `to-spec` with `specced` and `to-tickets`.
   It also repairs a published spec missing its planning queue label, without
   posting a duplicate. Existing specs are recognized by their complete template
   headings as well as the optional HTML marker.
3. `wayfinder-plan` applies the `to-tickets` skill to queued specs, creates
   `ready-for-agent` implementation sub-issues with native dependency edges,
   then replaces `to-tickets` (or legacy `to-plan`) with `planned`.
4. `wayfinder-delivery` reconciles planned parents every 30 minutes. It answers
   Copilot blocker questions, assigns one dependency-ready ticket at a time,
   requests GitHub Copilot code review after coding finishes, relays actionable
   findings back to `@copilot`, and merges only current-head reviewed work with
   successful required checks and no unresolved actionable threads or conflicts.
   It closes the parent only after every implementation ticket has merged work.
   Pull request/review events accelerate reconciliation; an activation condition
   excludes forks and non-`copilot/` heads. Only PRs linked to a planned parent's
   implementation tickets are eligible for writes.

Every stage has scheduled reconciliation and manual dispatch in addition to
events. This recovers missed events and GitHub's suppression of events caused by
`GITHUB_TOKEN` writes. `to-tickets` is the canonical queue label; `to-plan` is a
backward-compatible alias. Repeated runs reuse complete specs/tickets and wait
when Copilot or review is pending, rather than duplicating writes.
Runs of each stage are serialized to avoid simultaneous scheduled/event runs
creating duplicate specs, tickets, or assignments.

The workflows keep agent permissions read-only. All GitHub mutations use gh-aw
safe outputs. Copilot assignment requires an Actions secret named
`GH_AW_AGENT_TOKEN` with permission to assign the Copilot coding agent. Copilot
review uses the same secret through `add-reviewer` (reviewer `copilot`); no workflow
removes reviewer requests. Repository
branch protection remains authoritative: the delivery workflow never bypasses
failed checks, pending checks, conflicts, or protected-branch requirements.

Compile changes with:

```bash
gh aw compile wayfinder-to-spec wayfinder-spec wayfinder-plan wayfinder-delivery
python3 -m unittest discover -s .github/scripts -p 'test_wayfinder*.py'
```
