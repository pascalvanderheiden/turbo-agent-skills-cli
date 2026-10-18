---
description: Dispatch ordered implementation tickets to Copilot and finish completed wayfinder maps.
intent: Keep planned wayfinder delivery moving in dependency order until every coding ticket is merged and the parent map is closed.
on:
  issues:
    types: [closed, reopened, labeled, unlabeled, assigned, unassigned]
  issue_comment:
    types: [created]
  schedule: every 30 minutes
  workflow_dispatch:
permissions:
  actions: read
  checks: read
  copilot-requests: write
  contents: read
  issues: read
  pull-requests: read
tools:
  github:
    mode: gh-proxy
    toolsets: [default]
safe-outputs:
  assign-to-agent:
    allowed: [copilot]
    target: "*"
    max: 1
  add-comment:
    target: "*"
    max: 1
    hide-older-comments: false
  merge-pull-request:
    target: "*"
    allowed-branches: ["copilot/*", "copilot/**"]
    max: 1
  close-issue:
    target: "*"
    state-reason: completed
    max: 1
  noop:
    report-as-issue: false
    max: 1
network:
  allowed: []
---

# Deliver planned wayfinder maps

Reconcile one action for the oldest open issue labelled `wayfinder:map` and
`planned`. Use native sub-issues and native dependency edges as the source of
truth.

## Ordered ticket selection

Implementation tickets are the map's sub-issues labelled `ready-for-agent`.
Preserve the order in the map's plan-index comment. A ticket is frontier-ready
only when every `blocked_by` issue is closed.

Allow only one active coding ticket per map. An active ticket is an open child
already assigned to Copilot or linked to an open Copilot pull request. Never
assign a later ticket while an earlier ticket is open, blocked, assigned, or has
an unmerged pull request.

If no ticket is active, assign the first open frontier-ready ticket to Copilot.
Do not assign the parent map. Include custom instructions telling Copilot to:

- implement only that ticket and its acceptance criteria;
- preserve repository conventions;
- run the smallest relevant validation;
- open a non-draft pull request whose body closes the ticket and references the
  parent map; and
- ask a concrete question on the ticket if blocked.

## Copilot questions

Before assigning new work, inspect the active ticket's comments. If the latest
unanswered Copilot comment contains a concrete question, answer it in one issue
comment using only the map, spec, ticket, repository, and already-recorded
decisions. Do not invent product or architecture decisions. If evidence is
insufficient, state the exact missing decision and stop; do not assign or merge
other work in that run.

## Pull request completion

For the active ticket, identify a pull request authored by Copilot or from a
`copilot/` branch that explicitly closes the ticket and references the parent
map. Merge at most one pull request per run, without requesting or performing a
code review, only when all of these are true:

- it is open, non-draft, and GitHub reports it mergeable;
- all required checks have completed successfully;
- it has no merge conflicts;
- Copilot has no unanswered blocking question on the ticket or pull request;
- it changes only work attributable to the ticket; and
- branch protection and repository merge policy permit the merge.

Never bypass failed or pending checks, branch protection, or merge conflicts.
After a pull request is merged, close its ticket as completed only if GitHub did
not close it automatically. Perform at most one visible mutation category per
run: answer, assign, merge, close a ticket, or close the map.

## Parent completion

When every implementation sub-issue is closed and every pull request linked to
those tickets is merged or closed without unmerged work, close the map as
completed and use the close comment body for a concise shipped summary. Never
close a map while any child, dependency, or relevant pull request remains open.

If no transition is currently safe, call `noop` with the blocking condition.
