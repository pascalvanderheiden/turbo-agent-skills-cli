---
description: Label resolved wayfinder maps ready for specification.
intent: Keep wayfinder maps open until a spec is published by adding a single-use to-spec gate after all decision sub-issues are closed.
on:
  issues:
    types: [closed, edited, labeled, unlabeled, reopened]
  schedule: every 30 minutes
  workflow_dispatch:
concurrency:
  group: wayfinder-to-spec
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
safe-outputs:
  add-labels:
    target: "*"
    allowed: [to-spec]
    required-labels: ["wayfinder:map"]
    issues: true
    pull-requests: false
    create-if-missing: true
    max: 1
  noop:
    report-as-issue: false
    max: 1
network:
  allowed: []
---

# Label resolved wayfinder maps for specification

Find the oldest open issue labelled `wayfinder:map` that is not already labelled
`to-spec`, `specced`, `to-tickets`, `to-plan`, or `planned` and has all of its wayfinder decision
sub-issues closed.

A map becomes eligible for the specification gate when:

- it has at least one native wayfinder decision sub-issue, and every such
  sub-issue is closed (paginate the full list);
- the map itself is still open; and
- it does not already have a spec marker or a follow-on planning state.

When such a map is found, add the label `to-spec` and stop. Do not close the
map, do not create a planning ticket, and do not publish a spec from this
workflow.

If no eligible map is found, call `noop` with the first blocking condition and
make no visible write.

An explicit `to-spec` label on an ordinary issue is also supported by the
specification workflow; this automatic gate only selects resolved maps.
