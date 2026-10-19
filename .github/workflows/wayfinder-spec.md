---
description: Turn completed wayfinder maps into implementation specs.
intent: Convert a fully resolved wayfinder map into one traceable implementation spec without duplicating or prematurely planning work.
on:
  issues:
    types: [labeled, edited, reopened]
  schedule: every 30 minutes
  workflow_dispatch:
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
    required-labels: ["wayfinder:map", "to-spec"]
    max: 1
    hide-older-comments: false
  add-labels:
    target: "*"
    allowed: [specced, to-plan]
    required-labels: ["wayfinder:map", "to-spec"]
    issues: true
    pull-requests: false
    create-if-missing: true
    max: 1
  remove-labels:
    target: "*"
    allowed: [ready-for-agent, to-spec]
    required-labels: ["wayfinder:map"]
    max: 1
  noop:
    report-as-issue: false
    max: 1
network:
  allowed: []
---

# Spec completed wayfinder maps

Find the oldest open issue labelled `wayfinder:map` and `to-spec` that is ready to
become an implementation spec.

Use GitHub's native sub-issue relationships as the source of truth. A map is
ready only when:

- every wayfinder decision sub-issue is closed;
- `## Not yet specified` contains no unresolved in-scope fog;
- the map has enough recorded decisions to describe implementation and testing;
- it does not already contain a comment marker
  `<!-- wayfinder-spec:v1 -->`; and
- it is not already labelled `specced` or `planned`.

Read the map body, all resolution comments on its closed decision sub-issues,
the repository glossary and ADRs when present, and the minimum codebase context
needed to identify stable test seams. Apply the installed `to-spec` skill as the
specification format, with this workflow-specific publishing override:

- do not create a separate spec issue;
- do not interview the user;
- publish the complete spec as one comment on the wayfinder map;
- begin the comment with `<!-- wayfinder-spec:v1 -->`;
- include the chosen testing seams in `## Testing Decisions`; and
- after publishing, add both `specced` and `to-plan` to the map.

If the map has `ready-for-agent`, remove it because only implementation tickets,
not wayfinder decision maps, use that label.

If the map is still labelled `to-spec`, remove it once the spec has been
published so the label remains a single-use gate for the next specification run.

Process at most one map per run. If no map meets every condition, call `noop`
with the first unmet condition and make no visible write.
