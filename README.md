# TASC

TASC (Turbo Agent Skills CLI) helps you install, share, manage, watch-list, update, discover, search, evaluate, and check consistency across coding-assistant skills.

It is designed around a local-first Rust core that the CLI and Copilot SDK agent share, while delegating lifecycle tasks that GitHub already handles to `gh skill`.

## Goals

- Manage local agent-skill inventory and watchlists
- Evaluate skills against repeatable scenarios
- Detect consistency issues across installed skills
- Produce explainable insights without mutating skills silently
- Ship a Rust CLI and a GitHub Copilot agent from the same typed core

## Quick start

```bash
cargo install --path crates/tasc-cli

tasc skill list
tasc state status
```

## Command surface

```bash
tasc skill list
# Discover local Copilot skills and show which installations are tracked

tasc skill list --json
# Render the same inventory facts as versioned JSON

tasc skill adopt skill:file:///path/to/.github/skills/example
# Record an already-discovered installation without changing its files

tasc state status
# Show versioned Local State and discovery counts

tasc state status --json
# Render status facts as versioned JSON

tasc watch add owner/repo --dry-run
tasc watch add owner/repo --path skills/review/SKILL.md --yes
tasc watch list --json
tasc watch check
tasc watch check <watchlist-id>
tasc watch remove <watchlist-id> --yes
# Compare GitHub revisions without updating Installations

tasc eval run skill:file:///path/to/.github/skills/example --json
tasc eval run <installation-id> --revision <source-revision>
tasc eval list --json
tasc eval show <run-id>
# Validate local package metadata and inspect immutable Evaluation evidence

tasc consistency check <skill-id-1> <skill-id-2> --json
tasc consistency check --all-installed
tasc consistency list
tasc consistency show <check-id>
# Compare an explicit set of skills or opt in to all discovered skills

tasc source add github/awesome-copilot --dry-run
tasc source add github/awesome-copilot --yes
# Record normalized source metadata only: no fetch, install, or skill-content edit

tasc source list --json
tasc source show github/awesome-copilot
# Inspect recorded source IDs and GitHub locators (works offline)

tasc source capabilities --json
# Probe installed gh version and search/preview support

tasc skill search review --format json
# Also available as `tasc search review` or `tasc discover review`

tasc skill preview 'skill:https://github.com/owner/repo/skills/review/SKILL.md'
tasc skill preview skills/review/SKILL.md --source owner/repo --json
# Fetch upstream preview text without installing or executing skill files

tasc skill install review --source owner/repo --dry-run
tasc skill install review --source owner/repo --yes
# Plan and install a selected skill; outside a repository, also pass --scope user or project

tasc skill update <installation-id> --dry-run
tasc skill update <installation-id> --yes
tasc skill remove <installation-id> --yes
# Update or remove one recorded Installation
```

`skill list` discovers skills with a `SKILL.md` from `.github/skills` in the
current directory and `~/.copilot/skills`. Local State is stored at
`$XDG_STATE_HOME/tasc/state.json` or `~/.local/state/tasc/state.json`; set
`TASC_STATE_PATH` to use another file.

### GitHub discovery

Search and preview delegate to GitHub CLI's preview-stage `gh skill` commands.
The tested wire contract is `gh 2.101.0`: search must expose `--json` fields
`repo,skillName,namespace,description,stars,path` and `--limit`; preview must
accept a repository and skill path. TASC probes only the capability being used,
caches successful probes for the process, and never substitutes its own GitHub
API implementation when a capability is unavailable. Authenticate with
`gh auth login` or GitHub CLI's environment-based authentication.
Discovery is explicitly bound to `github.com`, irrespective of inherited
`GH_HOST` or GitHub CLI's configured default host.

Source commands manage TASC metadata, not a second GitHub catalog. `source show`
inspects that metadata; search discovers available skills and preview inspects
their contents. Adding a source does not scope subsequent global searches.
Source add requires `--yes` to write; `--dry-run` returns the normalized record
without writing. Existing version-1 Local State files remain readable.

Discovery results use a versioned envelope with `schemaVersion`, `command`,
`result`, and `warnings`. Both `--json` and `--format json` select JSON. Preview
preserves delegated content in `renderedText` rather than inventing structured
metadata. Human output escapes terminal control sequences. Diagnostics go to
stderr (structured with JSON output), with stable code, category, origin,
message, remediation, and candidate IDs for ambiguous selections.

Search fetches up to 100 results from the first delegated page. Ranking is
deterministic within those results: exact name, name substring, description
substring, then other delegated matches; ties use descending repository stars
and ascending Skill Identity. Each result includes a rank and explanation.
Duplicate identities are collapsed. Name-based preview requires one exact
match; ambiguity returns candidate IDs without selecting or previewing any.
Use a returned Skill Identity or an exact `SKILL.md` path with `--source` for
automation. Search and preview do not write Local State or installation files.

### Installation lifecycle

Install and update delegate to supported `gh skill` commands; removal deletes
only the recorded skill directory and then updates Local State. Every lifecycle
command supports `--dry-run`. Interactive mutations display a plan and prompt
for approval; non-interactive mutations require `--yes`. Updating or removing
locally modified content additionally requires `--force` and approval.

Successful installs and updates record the immutable `github-tree-sha` supplied
by `gh skill` (or a SHA-256 content digest when upstream metadata is absent),
the Adapter Target, and a local content digest. `skill list` reconciles tracked
Installations and reports modified, missing, or failed states; update plans
report when an Installation is outdated. A failed external operation remains
visible in Local State so it can be inspected and retried.

Each GitHub CLI invocation has a 30-second timeout and a 1-MiB limit per output
stream, with closed stdin, disabled prompts/pagers, and cancellation on Ctrl-C.
Missing executables, unsupported capabilities, authentication, command failures,
invalid output, timeout, and cancellation are distinct diagnostics. Exit codes
are `2` for usage/configuration, `3` for dependency/authentication, `4` for
operational failures, and `130` for cancellation. `--offline` rejects remote
discovery before invoking `gh`; local inventory, source metadata, and state
commands do not depend on GitHub CLI.

Watchlists are Local State metadata; each `watch check` writes an immutable
Watch Check artifact into a sibling `<state-stem>.watch-checks` directory.
Checks record prior and observed commit revisions, report unavailable sources
or GitHub CLI failures explicitly, and never update Installations. Repeated
observations of the same revision reuse the same immutable check reference. A
source selector matching multiple watched paths must use an exact Watchlist ID.

`eval run` performs deterministic package-only smoke validation of a local
`SKILL.md` (frontmatter `name` and `description`) without executing skill
instructions or invoking a model. It records identity, any known source revision
and target, package digest, analyzer configuration, policy, timestamps, outcomes,
and diagnostics in an immutable sibling `<state-stem>.evaluations` directory.
Scenario execution is `not_run`; `--waza` explicitly checks for the optional
Waza executable, reporting `inconclusive` when unavailable and `not_run` when
available because model-backed execution is outside this smoke slice. Run
artifacts are separate from Local State and can be listed or shown after the
original package changes or disappears.

`consistency check` requires at least two explicit selectors or the opt-in
`--all-installed` flag. It performs bounded, deterministic local analysis of
declared dependencies, ownership, precedence, assumptions, trigger phrases,
the current directory's `CONTEXT.md` glossary, explicit non-goals, and prior
Evaluation, Check, or Watch Artifacts. Lexical trigger overlap is a warning, not proof
of runtime collision; missing declarations are not treated as conflicts.
Findings identify deterministic evidence, confidence, severity, and revision
status. Inferred evidence is explicitly `not_run`; no model or live composition
probe is invoked. Each run is an immutable artifact in a sibling
`<state-stem>.consistency-checks` directory, separate from Local State.

`insight generate` turns observed Installation lifecycle states, the latest
Evaluation per Skill Identity, and the latest Consistency findings into ranked,
immutable Insight snapshots. Priority is an explainable deterministic ordering
of blocking impact, evidence confidence, urgency, affected Installations,
freshness, and stable Insight ID; it is not a universal quality score.
Impact orders `blocking` above `warning` above `info`; confidence preserves the
source score (lifecycle and Evaluation observations use 100, inferred lifecycle
evidence uses 60, and Consistency uses its finding score). Urgency is a separate
deterministic 0–3 source-state factor (no unsupported deadlines are inferred),
breadth counts distinct affected Installations, and freshness falls by one point
per evidence age-day to zero at 100 days; lifecycle observations without a
source timestamp are treated as current.
`insight list` and `insight show` expose each snapshot's contributing evidence,
revision, priority factors, and lifecycle status. Changed evidence creates a new
snapshot and leaves its prior snapshot visible as stale. Suppressed Insights
are omitted from the default actionable list; use `insight list --all` or
`insight show` to inspect their retained evidence and audit history.

`insight status` records acknowledged, snoozed, or resolved lifecycle events
without rewriting snapshots. `insight suppress` requires a reason and expiration
and can target one contributing evidence item with `--evidence`; each
append-only suppression records the exact evidence version. Suppressions do not
delete or modify source evidence, expire automatically, and can be audited or
ended with `insight unsuppress`. Insight snapshots and lifecycle events are
stored in sibling `<state-stem>.insights` and
`<state-stem>.insight-lifecycle` directories.

`insight propose <id>` renders a bounded Remediation Proposal with scope, risk,
prerequisites, reversibility, affected evidence, and expected verification.
Proposals are informational only: no Insight command applies a proposal or
edits an Installation or Skill Workspace.

## Architecture

- `tasc-core` holds the canonical language model and state types
- `tasc-agent` provides the shared inventory, adoption, adapter, and state service
- `tasc-cli` provides the command-line surface

## Release and delivery

Stable releases build reproducible Linux x64/ARM64, macOS Intel/Apple Silicon, and Windows x64 archives from protected SemVer tags. Publication requires passing workspace tests and packaging/integrity checks plus approval in the `release-approval` environment. Releases include SHA-256 checksums, a CycloneDX SBOM, build provenance, and a separate SBOM attestation; configure repository tag rulesets and immutable releases as described in the [release runbook](.github/RELEASE.md).

The [versioned GitHub Pages documentation](https://pascalvanderheiden.github.io/turbo-agent-skills-cli/) includes installation and verification, lifecycle safety, feature guides, architecture, troubleshooting, and release recovery. TASC does not self-update; users manually download, verify, and replace binaries.

## Related work

- GitHub CLI skill lifecycle: delegated to `gh skill`
- Waza: evaluation foundation
- Kratos patterns: used as reference for consistency checks and repair guidance
