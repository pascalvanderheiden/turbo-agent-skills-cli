---
description: Split an approved wayfinder spec into ordered implementation tickets.
intent: Publish a dependency-aware implementation plan whose tickets are traceable to the completed wayfinder map and immediately usable by coding agents.
on:
  label_command:
    name: to-plan
    events: [issues]
    remove_label: false
  workflow_dispatch:
concurrency:
  group: wayfinder-plan
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
  - .agents/skills/to-tickets
safe-outputs:
  create-issue:
    labels: [ready-for-agent]
    allowed-labels: [ready-for-agent]
    require-temporary-id: true
    deduplicate-by-title: true
    max: 25
    footer: false
  replace-label:
    target: "triggering"
    allowed-add: [planned]
    allowed-remove: [to-plan]
    required-labels: ["wayfinder:map", specced, to-plan]
    max: 1
  add-comment:
    target: "triggering"
    required-labels: ["wayfinder:map", specced]
    max: 1
  noop:
    report-as-issue: false
    max: 1
network:
  allowed: []
---

# Plan a specced wayfinder map

Act only on the triggering issue. It must be an open issue labelled
`wayfinder:map`, `specced`, and `to-plan`, with one comment beginning
`<!-- wayfinder-spec:v1 -->`.

Apply the installed `to-tickets` skill to that spec. For this automated path,
the `specced` plus `to-plan` labels are the user's approval to publish the
breakdown, so do not pause for a quiz. If the spec leaves ticket granularity,
acceptance criteria, or dependency edges genuinely ambiguous, call `noop`
instead of guessing.

Create tracer-bullet implementation tickets in dependency order. Every ticket
must:

- use a unique temporary ID matching `aw_[A-Za-z0-9_]{3,12}`;
- set `parent` to the triggering wayfinder map so GitHub records a native
  sub-issue relationship;
- include `## Parent`, `## What to build`, and `## Acceptance criteria`;
- be small enough for one fresh coding-agent context;
- carry the fixed `ready-for-agent` label;
- use `blocked_by` with exact temporary IDs (for example `aw_shell`, never
  `#aw_shell`) or existing issue numbers for every true predecessor, so GitHub
  records native dependency edges; and
- avoid implementation file paths unless a prototype made one an explicit
  decision.

Search existing sub-issues before creating anything. If an equivalent ticket
already exists, reuse it and never create a duplicate.

Emit every `create-issue` output first. Then emit one concise plan-index comment
to the map, listing tickets in execution order with their blockers. Emit the
`replace-label` output last, replacing `to-plan` with `planned`. Never emit that
label transition if any ticket or dependency edge is omitted from the outputs.

Do not assign Copilot, merge pull requests, or close the map in this workflow.
If no ticket needs creation and the existing plan is complete, only post the
index if missing and transition the label. Otherwise call `noop`.
