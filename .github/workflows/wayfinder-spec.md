---
description: Spec labelled issues and hand completed specifications to ticket planning.
intent: Convert explicitly queued issues or resolved wayfinder maps into one traceable implementation spec and recover incomplete planning handoffs without duplicating specs.
on:
  issues:
    types: [labeled, edited, reopened]
  issue_comment:
    types: [created]
  schedule: every 30 minutes
  workflow_dispatch:
concurrency:
  group: wayfinder-spec
  queue: max
  job-discriminator: ${{ github.run_id }}
permissions:
  copilot-requests: write
  contents: read
  issues: read
  pull-requests: read
tools:
  github:
    mode: gh-proxy
    toolsets: [default]
skills:
  - .agents/skills/to-spec
safe-outputs:
  add-comment:
    target: "*"
    max: 1
    hide-older-comments: false
  add-labels:
    target: "*"
    allowed: [specced, to-tickets]
    issues: true
    pull-requests: false
    create-if-missing: true
    max: 2
  remove-labels:
    target: "*"
    allowed: [ready-for-agent, to-spec, to-plan]
    max: 3
  noop:
    report-as-issue: false
    max: 1
network:
  allowed: []
---

# Spec queued issues and recover planning handoffs

Reconcile the oldest eligible open issue in this repository. Paginate results.
Candidates are issues labelled `to-spec`, or `specced` issues missing all of
`to-tickets`, `to-plan`, and `planned`. Do not require `wayfinder:map` for an explicit
`to-spec` request. Ignore unrelated issues and pull requests.

For a wayfinder map, use native sub-issue relationships as the source of truth.
It is ready only when every wayfinder decision child is closed. Read the map's
remaining fog in context: a statement that no issue blocks the specification is
not unresolved fog. Do not treat the mere presence of `## Not yet specified` as
a blocker. Actual unresolved product choices block synthesis; report the exact
missing decision once on the issue rather than repeatedly silently skipping it.

Before writing, inspect the issue body and all comments for an existing complete
spec. Recognize `<!-- wayfinder-spec:v1 -->` OR the complete `to-spec` template:
`## Problem Statement`, `## Solution`, `## User Stories`,
`## Implementation Decisions`, `## Testing Decisions`, `## Out of Scope`, and
`## Further Notes`. GitHub may strip HTML comment markers, so the marker is not
the only identity. Do not mistake a decision-resolution comment for that spec.

If a complete spec already exists, do not publish another. Repair the handoff by
adding `specced` and `to-tickets` and removing stale queue labels, then stop.
Never requeue an issue labelled `planned`.

Read the map body, all resolution comments on its closed decision sub-issues,
the repository glossary and ADRs when present, and the minimum codebase context
needed to identify stable test seams. Apply the installed `to-spec` skill as the
specification format, with this workflow-specific publishing override:

- do not create a separate spec issue or add `ready-for-agent` to the parent;
- do not interview the user;
- publish the complete spec as one comment on the selected issue/map;
- begin the comment with `<!-- wayfinder-spec:v1 -->`;
- include the chosen testing seams in `## Testing Decisions`; and
- emit the spec comment first, then one `add-labels` operation adding BOTH
  `specced` and `to-tickets` (the limit is two labels, not one). Finally remove
  `to-spec`, `to-plan`, and `ready-for-agent` when present. Never emit `specced`
  without the ticket queue.

Only mutate an issue selected by the eligibility rules above. A `specced` issue
with no complete spec must be reported as inconsistent once, not sent to
planning or repeatedly commented on.

Process at most one issue per run. If no issue meets every condition, call `noop`
with the first unmet condition and make no visible write.
