---
description: Split queued specifications into ordered implementation tickets.
intent: Publish a dependency-aware implementation plan whose tickets are traceable to the completed wayfinder map and immediately usable by coding agents.
on:
  issues:
    types: [labeled, edited, reopened]
  issue_comment:
    types: [created]
  schedule: every 30 minutes
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
    target: "*"
    allowed-add: [planned]
    allowed-remove: [to-tickets, to-plan]
    required-labels: [specced]
    max: 1
  remove-labels:
    target: "*"
    allowed: [to-tickets, to-plan]
    required-labels: [planned]
    max: 2
  add-comment:
    target: "*"
    required-labels: [specced]
    max: 1
  noop:
    report-as-issue: false
    max: 1
network:
  allowed: []
---

# Plan a specced wayfinder map

Find the oldest open issue labelled `specced` and either `to-tickets` or the
legacy alias `to-plan`, excluding `planned`. Paginate candidates and native
sub-issues. Do not require a triggering issue: schedules and manual dispatch
must recover missed or bot-generated label events. Ordinary issues as well as
wayfinder maps are supported.

Read the complete spec from its body/comments. Recognize the marker
`<!-- wayfinder-spec:v1 -->` OR all seven `to-spec` template headings (Problem
Statement, Solution, User Stories, Implementation Decisions, Testing Decisions,
Out of Scope, Further Notes); HTML markers can be stripped during publishing.

Apply the installed `to-tickets` skill to that spec. For this automated path,
the `specced` plus `to-tickets` (or legacy `to-plan`) labels are approval to publish the
breakdown, so do not pause for a quiz. If the spec leaves ticket granularity,
acceptance criteria, or dependency edges genuinely ambiguous, call `noop`
instead of guessing.

Create tracer-bullet implementation tickets in dependency order. Every ticket
must:

- use a unique temporary ID matching `aw_[A-Za-z0-9_]{3,12}`;
- set `parent` to the selected issue/map so GitHub records a native
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
already exists, reuse it and never create a duplicate. Include those existing
ticket numbers in the index. Never transition if reused tickets are missing
required native dependency edges; report the exact repair needed.

Emit every `create-issue` output first. Then emit one concise plan-index comment
to the parent, listing tickets in execution order with their blockers and the
stable marker `wayfinder-plan:v1`. Emit the
`replace-label` output after all creation/index outputs, replacing the present
queue label (`to-tickets`, or `to-plan` for legacy issues) with `planned`.
The replacement tool accepts ONE label to add and ONE to remove, not arrays.
If both queue labels were present, remove the remaining one only after that
transition. Never emit that
label transition if any ticket or dependency edge is omitted from the outputs.

Do not assign Copilot, merge pull requests, or close the map in this workflow.
If no ticket needs creation and the existing plan is complete, only post the
index if missing and transition the label. Otherwise call `noop`.
