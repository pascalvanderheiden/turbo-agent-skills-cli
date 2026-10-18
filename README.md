# TASC

TASC (Turbo Agent Skills CLI) helps you install, share, manage, watch-list, update, discover, search, evaluate, check consistency, and analyze security risks across coding-assistant skills.

It is designed around a local-first Rust core that the CLI and Copilot SDK agent share, while delegating lifecycle tasks that GitHub already handles to `gh skill`.

## Goals

- Manage local agent-skill inventory and watchlists
- Evaluate skills against repeatable scenarios
- Detect consistency issues across installed skills
- Detect exposed secrets and risky skill content with local static Security Checks
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

tasc skill track <installation-id> --json
tasc skill track --all --json
# Track selected installations or all currently untracked installations

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

tasc skill create --workspace ./skill-workspace --name review-helper
# Answer TASC's deterministic authoring questions; no files are written until approval

tasc skill create --workspace ./skill-workspace --name review-helper \
  --purpose "Review Rust changes" \
  --description "Review Rust changes for correctness." \
  --triggers "When a user requests a Rust code review." \
  --non-goals "Do not run deployment commands." --dry-run
tasc skill create --workspace ./skill-workspace --name review-helper \
  --purpose "Review Rust changes" \
  --description "Review Rust changes for correctness." \
  --triggers "When a user requests a Rust code review." \
  --non-goals "Do not run deployment commands." --yes
tasc skill create --workspace ./skill-workspace --name review-helper \
  --purpose "Review Rust changes" \
  --description "Review Rust changes for correctness." \
  --triggers "When a user requests a Rust code review." \
  --non-goals "Do not run deployment commands." \
  --category coding --category testing --yes
tasc skill drafts --workspace ./skill-workspace --json
tasc skill resume <draft-id> --workspace ./skill-workspace --json
tasc skill validate <draft-id> --workspace ./skill-workspace --json
tasc skill remediate <draft-id> --workspace ./skill-workspace \
  --description "Review Rust changes for correctness." --dry-run
# Create, resume, inspect, validate, and propose bounded Workspace-only repairs
tasc skill categories --json
tasc bundle validate ./bundle.json --json
# Categories are descriptive metadata; bundle validation never deploys

tasc consistency check <skill-id-1> <skill-id-2> --json
tasc consistency check --all-installed
tasc consistency list
tasc consistency show <check-id>
# Compare an explicit set of skills or opt in to all discovered skills

tasc security check <skill-id> --json
tasc security check /path/to/skill-workspace --fail-on high
tasc security check --all-installed
tasc security list
tasc security show <check-id>
tasc security suppress <finding-fingerprint> --reason "Reviewed" --expires-in-days 30
tasc security unsuppress <finding-fingerprint> --reason "Reconsidered"
# Analyze local content only, with narrow auditable suppressions and optional policy exits

tasc source add github/awesome-copilot --dry-run
tasc source add github/awesome-copilot --yes
# Record normalized source metadata only: no fetch, install, or skill-content edit

tasc source list --json
tasc source show github/awesome-copilot
# Inspect recorded source IDs and GitHub locators (works offline)

tasc source capabilities --json
# Probe installed gh version and search/preview support

tasc skill search review --format json
tasc discover "review code changes" --format json
# Strict lookup and intent-oriented discovery have separate result contracts

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
current directory and `~/.copilot/skills`. It projects a stable action ID,
lifecycle, Tracking, Watchlist, scope, source, provenance evidence, and a
description excerpt for each Installation or watched-only interest. Use
`--sort`, `--offset`, `--limit`, `--status`, `--scope`, `--assistant`,
`--provenance`, `--tracking`, `--watch`, or `--detail <stable-id>` to select
rows; display row numbers are local to the current view.

On an interactive first inventory of an untracked installation cohort, TASC
offers to track all, select installations, or defer. Deferral is remembered for
that cohort; discovering another installation presents a new prompt. `skill track`
can explicitly adopt one or more stable Installation IDs, or `--all` to track
every untracked installation in the selected Profile. Tracking only records
metadata and never reinstalls or edits skill files. Tracking is scoped to both
the selected Profile and Adapter Target.

Project operations select `.tasc/state.json` when that project Profile exists;
otherwise they use the user Profile at `$XDG_STATE_HOME/tasc/state.json` or
`~/.local/state/tasc/state.json`. Profiles are not merged. Set
`TASC_STATE_PATH` to select a custom state file. Existing version-1 Local State
is migrated atomically to version 2 when loaded; malformed or unsupported
versions are left untouched.

### Audit and statistics

`tasc audit` checks the selected Profile's Local State, installations, Adapter
Targets, references, and existing Evaluation, Consistency, and Security evidence.
It is read-only: version-1 Local State is interpreted in memory, and immutable
artifacts are never refreshed or rewritten. Evidence freshness compares recorded
source revisions or content digests with the currently available inputs; age
alone does not make evidence stale. Findings are classified as `error`,
`warning`, or `optimization`.

`tasc stats` reports the same profile-scoped inventory and summarizes scope,
Tracking, Watchlist, provenance, lifecycle, repositories, audit findings, and
comparable evidence outcomes. Both commands support `--json` and `--format json`;
their version-1 JSON reports include the command, generation time, Profile,
summary, typed findings or outcomes, diagnostics, and exit code.

`tasc doctor` aliases `tasc audit`, and `tasc statistics` aliases `tasc stats`.
Audit exits `0` when there are no error findings (warnings and optimizations are
non-fatal), `1` when error findings exist, and `4` when required profile or
evidence inputs cannot be inspected. Stats exits `0` when inputs are available
and `4` when an operational input is unavailable.

### Conversational agent

`tasc agent` starts a Copilot SDK session. Install and authenticate the GitHub
Copilot CLI first, or set `COPILOT_CLI_PATH` to its executable. In a terminal,
it shows the Turbo banner and a status line with GitHub CLI authentication,
the selected model, SDK-reported token usage when available, and inventory,
Tracking, and Watchlist counts. The banner and status are omitted in
non-interactive mode. `NO_COLOR` disables ANSI styling; `--format json` emits
versioned JSON events.

```bash
tasc agent
```

The agent accepts natural-language requests and noun-first Slash Commands such
as `/skill list`, `/evaluation run <skill>`, `/consistency check <skills>`,
`/security check <skills>`, `/audit`, `/stats`, `/profile status`, `/bundle
validate <manifest>`. Use `/model` to list
SDK-exposed models, `/model <id>` for a session-only override, and
`/model profile <id>` to save a Profile preference. `auto` is the default.
`/help` lists the shell commands.

The saved model preference is stored separately from Local State alongside the
selected Profile's state file. Command Skill management commands are recognized
by the shell, but operations requiring a Command Skill management service are
reported unavailable until that service is provided.

Lifecycle requests can prepare plans but cannot mutate installations until
explicitly approved. Review the proposed operation and enter `/approve <token>`
to authorize it, or `/deny <token>` to decline; the existing `:approve`,
`:deny`, and `:quit` spellings remain aliases. Approval tokens expire after
five minutes and can be used only once. `/cancel` or Ctrl-C stops active work.

The SDK receives only bounded summaries of local results, not skill contents,
local filesystem paths, or detailed Security evidence. Session storage and SDK
session telemetry are disabled; TASC does not retain the conversation. The
Copilot CLI still needs model access even with `--offline`; that option disables
GitHub-backed TASC tools such as remote skill search and update planning, not
the SDK’s connection to Copilot.

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
inspects that metadata; Search discovers exact GitHub matches and preview
inspects their contents. Adding a source does not scope subsequent global
Searches. Source add requires `--yes` to write; `--dry-run` returns the
normalized record without writing.

Discovery results use a versioned envelope with `schemaVersion`, `command`,
`result`, and `warnings`. Both `--json` and `--format json` select JSON. Preview
preserves delegated content in `renderedText` rather than inventing structured
metadata. Human output escapes terminal control sequences. Diagnostics go to
stderr (structured with JSON output), with stable code, category, origin,
message, remediation, and candidate IDs for ambiguous selections.

`search` is strict lookup, not intent inference. It fetches up to 100 results
from the first delegated GitHub page and retains exact names, paths, identities,
or complete keyword matches in returned metadata. Results and duplicate
identity selection use ascending Skill Identity, independently of GitHub's
result order or star counts. Name-based preview requires one exact match;
ambiguity returns candidate IDs without selecting or previewing any. Use a
returned Skill Identity or an exact `SKILL.md` path with `--source` for
automation.

`discover` ranks local tracked and untracked skills together with remote GitHub
matches. It uses name and description intent, declared `metadata.categories`
and `metadata.capabilities`, and source provenance evidence; results include
stable Skill Identity references and the match reasons and provenance
confidence behind the ranking. Recorded Profile sources and Watchlist sources
are exposed as eligible sources. With `--offline`, local results are still
returned and `remoteExpansion` is `unavailable` with a diagnostic instead of
silently presenting a local-only result as complete. A failed remote expansion
also preserves local results and reports its diagnostic.

Pass `--rerank` to explicitly ask the Copilot CLI to rerank the same candidates.
This sends candidate names, descriptions, Categories, capabilities, and
provenance metadata to the model, but not skill contents or local paths.
Deterministic ranks remain in `rank`; model ordering is annotated in
`modelRank` and `reranking`. Invalid or unavailable model output leaves the
deterministic order intact and is reported in `diagnostics`. Search and
Discover do not write Local State or installation files.

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
An exact skill Watchlist entry is retired when that skill is explicitly tracked
or installed; a broader source Watchlist remains and excludes installed matches.

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

`skill create` is guided Skill Authoring, not Installation. It requires an
explicit existing Skill Workspace, inspects the nearest local `CONTEXT.md` for
headings and a digest, and asks for the skill name, purpose, description,
activation guidance, and non-goals that were not supplied. It uses TASC-owned
grilling questions with a deterministic offline path, previews the proposed
`SKILL.md`, and requires interactive approval or `--yes` before writing.
`--dry-run` never writes. Draft records are durable in the selected Workspace's
`.tasc/drafts` directory; `skill drafts` requires `--workspace`.
Incomplete drafts are marked in their package and are excluded from ready-skill
discovery. Structured authoring answers are
redacted before persistence; no interview transcript is retained. `skill resume`
reads the saved answers for the selected Workspace without writing files.
Validation separates readiness-blocking normative diagnostics from optional
quality guidance. Failed drafts remain available for correction. The bounded
`skill remediate` action only adds a missing
description, records a pre-change copy under `.tasc-recovery`, validates the
result, and restores the original file if validation fails. Every authoring write,
including resumable Draft records, is confined to the selected Workspace; an
Installation or discovered installed copy is never an authoring target.

A ready draft does not install, evaluate, export, share, commit, push, or publish.
Each such follow-up requires a separate explicit user action. Authoring does not
create commits or push files.

`skill categories` lists TASC's descriptive category vocabulary. Categories may
be attached to a draft with repeatable `--category` options and are stored as
skill metadata; they do not change installation, tracking, or execution behavior.
`bundle validate` reads a versioned JSON manifest and checks Skill Identity and
source consistency, dependency ordering, target intent, and approval-required
policy. It is read-only: bundle publication and deployment are not implemented
by this command, and validation never installs anything.

Bundle manifests use this versioned shape:

```json
{
  "schemaVersion": 1,
  "id": "review-workflow",
  "name": "Review workflow",
  "version": "1.0.0",
  "description": "Skills for a review workflow.",
  "categories": ["coding"],
  "skills": [
    {
      "identity": {
        "id": "skill:github.com/example/skills/review",
        "name": "review",
        "source": "github.com/example/skills",
        "relative_path": "review"
      },
      "order": 0,
      "dependencies": []
    }
  ],
  "targetIntent": [{ "assistant": "copilot", "scope": "project" }],
  "policy": {
    "installRequiresApproval": true,
    "publishRequiresApproval": true
  }
}
```

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

`security check` requires one or more explicit Skill Identity, Installation, or
Skill Workspace selectors, or the opt-in `--all-installed` flag. The static
analyzers check local content for exposed secrets, unsafe permissions and
instructions, injection or exfiltration patterns, risky executable content,
supply-chain indicators, and suspicious references. Evidence excerpts redact
credential-shaped values; severity and confidence remain independent. A
revision-pinned immutable artifact records the content digest, findings,
diagnostics, and any partial-analysis status in `<state-stem>.security-checks`.
No skill content is executed and external references are never fetched. Dynamic
execution and external reputation checks are explicitly recorded as not run.
`--fail-on <severity>` is opt-in and blocks only deterministic findings with at
least 90 confidence and the selected severity or higher.

`security suppress` targets one exact finding fingerprint, requires a reason,
and expires after 1–365 days; `security unsuppress` ends the active suppression
with an audit reason. Suppressions do not rewrite Security Check Artifacts or
remove findings. Security recommendations are informational and never modify a
Skill Workspace.

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
