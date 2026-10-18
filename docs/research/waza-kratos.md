# Research: Waza (Evaluation foundation) and Kratos Agent (Consistency reference)

**Ticket:** #4 — "What capabilities and extension points in Microsoft Waza can support TASC
Agent Skill Evaluations, and what reusable analysis patterns from Kratos Agent can inform
cross-skill Consistency Checks?" (part of #1, blocks #9 and #10)

**Scope stance (per ticket):** Waza is a **required** foundation for TASC's Evaluation
capability. Kratos Agent is a **reference implementation only** — its patterns are studied
for design ideas; **TASC must not take it on as a runtime dependency.**

---

## 1. Summary

- **Waza** (`microsoft/waza`) is Microsoft's open-source Go CLI "for evaluating agent
  workflows, skills, and custom agents" — it is the authoritative, actively maintained
  evaluation engine for `SKILL.md`-shaped agent skills, with a published CLI (GitHub's
  latest-release URL resolves to `azd-ext-microsoft-azd-waza_0.38.10` as of
  2026-10-16), a documented eval-spec schema, pluggable graders, an MCP server mode, and
  a compliance-scoring subsystem (`waza dev`/`waza check`) purpose-built for skill
  frontmatter quality. The documentation also labels some `main` features as upcoming,
  so TASC must check each integration point against its pinned Waza release.
- **Kratos Agent** — the repository that matches the ticket's description (multi-agent
  consistency analysis, conflict/precedence detection, remediation) is
  **`LizardLiang/kratos`** ("The God of War — Multi-agent orchestrator plugin for Claude
  Code"), not any of the many same-named Go microservice frameworks or
  identity-management products that also use the name "Kratos". It implements a
  9-stage, 19-agent pipeline with optional Stage 0 research, an explicit **Agent Handoff
  Specification**, disjoint verdict vocabularies, scope-fence fields on every spawn,
  and a machine-checkable `status.json` conflict detector. These are transferable
  *patterns*, not code, for TASC's cross-skill Consistency Check (#10).
- Both repositories are MIT-licensed, current as of **2026-10-16**, and both are
  small/young enough that line-level citations below should be treated as a point-in-time
  snapshot (commits `d457425` for Waza, `0122006` for Kratos).

---

## 2. Microsoft Waza — authoritative sources

| Source | URL |
|---|---|
| Repository (canonical) | https://github.com/microsoft/waza |
| Docs site | https://microsoft.github.io/waza/ |
| Concepts/learning path | https://github.com/microsoft/waza/blob/main/site/src/content/docs/concepts.mdx |
| `eval.yaml` schema reference | https://github.com/microsoft/waza/blob/main/site/src/content/docs/reference/schema.mdx |
| CLI reference | https://github.com/microsoft/waza/blob/main/site/src/content/docs/reference/cli.mdx |
| Graders guide | https://github.com/microsoft/waza/blob/main/site/src/content/docs/guides/graders.mdx |
| Spec-verify guide | https://github.com/microsoft/waza/blob/main/site/src/content/docs/guides/spec-verify.mdx |
| `waza` skill definition (dogfood) | https://github.com/microsoft/waza/blob/main/skills/waza/SKILL.md |
| Releases | https://github.com/microsoft/waza/releases |
| Azure Developer CLI extension registry | https://raw.githubusercontent.com/microsoft/waza/main/registry.json |
| License | [MIT](https://github.com/microsoft/waza/blob/main/LICENSE) |

Repository metadata was fetched via `raw.githubusercontent.com` (public, unauthenticated)
because the `microsoft` GitHub organization enforces SAML SSO that blocked the `gh`
CLI/API in this sandbox (`gh repo view microsoft/waza` → *"Resource protected by
organization SAML enforcement"*). The raw README and a shallow git clone (commit
`d457425953e609d60c4b5059d9f1117d0ac8494c`, 2026-10-16) were used instead for all
source-level citations below; verify current content directly in-browser when doing
further follow-up, since SAML enforcement may also affect contributors' own sessions.

### 2.1 What Waza evaluates and how (core model)

`microsoft/waza:site/src/content/docs/concepts.mdx` defines the vocabulary TASC's
Evaluation contract (#9) should align with:

- **Suite** (`eval.yaml`) → selects task files, runtime settings, graders.
- **Task** → prompt + inputs + expected behavior, optional follow-ups/checkpoints.
- **Trial** → one measured run of a task (`trials_per_task` for independent repeats).
- **Attempt** → one execution within a trial (`max_attempts` governs in-trial retries;
  only the final attempt's verdict is retained — "a later success can replace an earlier
  failure").
- **Scenario** → a workflow under eval schema `2.0` (task/result formats stay on a
  separately versioned `1.x`); legacy skill suites still work.
- **Skills** (`SKILL.md`) / **custom agents** (`.agent.md`) are optional instruction/tool
  context — *"not substitutes for the task's checks."*

Executors are explicitly **not** a quality guarantee: `mock` returns a canned response to
validate harness wiring only, `copilot-sdk` runs a real agent
(`microsoft/waza:site/src/content/docs/concepts.mdx` — "Executors are not dependencies").
This maps to Waza's own repeated caveat that "mock success checks the harness, not agent
quality or live service safety" (README). TASC should carry this distinction into its own
Evaluation contract language so users don't conflate a green `waza run --executor mock`
with a quality signal.

### 2.2 CLI surface (extension points TASC can drive)

`microsoft/waza:cmd/waza/root.go` and sibling `cmd_*.go` files expose (verified against
the cloned source tree, package imports under `github.com/microsoft/waza/cmd/waza/...`):

- `waza init [dir]` — scaffold an eval project (`.waza.yaml`, `evals/<skill>/`).
- `waza new skill <name>` / `waza new eval <skill>` / `waza new eval --scenario` —
  scaffolding (`internal/scaffold`, `internal/generate`, `internal/wizard`).
- `waza run <eval.yaml>` — execute a benchmark (`--parallel`, `--workers`, `--task`,
  `--transcript-dir`).
- `waza check <skill-path>` and `waza dev <skill-path>` — compliance scoring
  (`internal/checks`, `internal/scoring`) — see §2.3.
- `waza generate <SKILL.md>` / `waza suggest <skill>` — derive an eval suite from an
  existing skill (`internal/generate`, `internal/suggest`).
- `waza spec verify <skill> <eval.yaml>` — verify eval coverage against SKILL.md
  requirements (`internal/specverify`); supports `--fail --format github-actions` for CI
  gating.
- `waza get <eval.yaml>` / `waza registry add|search` — resolve and pin remote shared
  graders into `waza.lock` (`internal/registry`; design notes in
  `microsoft/waza:docs/research/waza-eval-registry-design.md`, describing a **federated,
  Git-ref-based package model** — "Repos as packages... Indexes are discovery metadata,
  not the package source of truth" — deliberately avoiding a single central registry).
- `waza assure <eval.yaml> --references <labels.json>` — offline "challenge the graders"
  mode using human-labelled references (`internal/assurance`).
- `waza compare <r1.json> <r2.json> ...` — cross-model/cross-run result diffing.
- `waza gate` — documented, stable exit-code matrix for CI regression gating
  (`microsoft/waza:cmd/waza/cmd_gate.go`).
- `waza serve` — starts either an HTTP dashboard or a **JSON-RPC 2.0 server** over TCP or
  stdio (`microsoft/waza:cmd/waza/cmd_serve.go:17-50`), exposing `eval.run`, `eval.list`,
  `eval.get`, `eval.validate`, `task.list`, `task.get`, `run.status`, `run.cancel` — a
  ready-made IDE/automation integration point.
- `waza migrate` — migrates a schema artifact to the current schema version
  (`microsoft/waza:cmd/waza/cmd_migrate.go`), relevant to TASC's own forward-compat story
  for stored Evaluation/Insight records.

### 2.3 Skill-quality scoring (`waza dev` / `waza check`) — most directly reusable subsystem

`microsoft/waza:internal/checks/compliance_score.go:9-43` defines
`ComplianceScoreChecker`, which wraps a pluggable `scoring.Scorer` interface
(`HeuristicScorer` is the default) and returns a `ComplianceScoreData{Score, Level}`
with pass/fail gated at `AdherenceMediumHigh`. The scoring rubric, documented in
`microsoft/waza:skills/waza/SKILL.md`, is:

| Compliance level | Criteria |
|---|---|
| Low | description < 150 chars or no trigger phrases |
| Medium | 150+ chars, has `USE FOR:`-style triggers |
| Medium-High | + anti-triggers (`DO NOT USE FOR:`) |
| High | + routing markers (`**WORKFLOW SKILL**`, `INVOKES:`, `FOR SINGLE OPERATIONS:`) |

Additional checks: description length bounds (150–1024 chars), token budget (500 soft /
5000 hard limit — `internal/checks/token_budget.go`, `internal/checks/token_limits.go`),
and link/spec validation (`internal/checks/spec_checks.go`,
`internal/checks/eval_suite.go`). This is Waza's single most relevant subsystem for
TASC's own per-skill **Evaluation** quality gate. However, `internal/scoring` is a Go
`internal` package, not a supported external plugin API: TASC should consume
`waza check` output and reuse the rubric vocabulary, not import or depend on the
`Scorer` interface.

### 2.4 Trigger-phrase testing — directly useful for cross-skill overlap/consistency signals

`microsoft/waza:internal/trigger/{spec.go,discover.go,runner.go}` implements
`trigger_tests.yaml`: a skill declares `should_trigger_prompts` and
`should_not_trigger_prompts`, and the runner (`runner.go:60-75`) executes each against a
real or mocked agent engine and classifies hit/miss, producing `models.TriggerMetrics`
(precision/recall-style trigger accuracy). This is the closest thing Waza has to
cross-skill awareness today — it is scoped to **one skill's own** trigger boundary, not
multi-skill overlap, but the mechanism (prompt → should-this-skill-fire? → classification
metrics) is a strong template for a TASC Consistency Check. TASC can orchestrate the same
prompt corpus through separate Waza evaluations for multiple Installed Skills, then
compare which skills fire, without modifying Waza's internal trigger runner.

### 2.5 Multi-skill composition support that already exists

`microsoft/waza:examples/required-skills-demo/README.md` documents `required_skills` in
`eval.yaml`'s `config` block: an orchestration skill (e.g. `azure-deploy`) can declare
dependencies on other skills (`azure-prepare`, `azure-validate`); Waza performs **preflight
validation** that all declared skill directories are present before the eval runs, failing
fast with a clear "missing" list rather than a confusing runtime failure. This is evidence
that Waza already has an extension point for inter-skill dependency declarations — TASC's
Consistency Check (#10) could consume or validate against the same `required_skills`
convention so skill-level dependency graphs aren't invented twice.

### 2.6 Grader / extension architecture

`microsoft/waza:internal/graders/grader.go:16-27` defines the core `Grader` interface
(`Name() string`, `Kind() models.GraderKind`, `Grade(ctx, *Context) (*models.GraderResults,
error)`), with a rich `Context` struct (transcript, output, workspace files, tool
events, skill invocations, baseline output for pairwise comparisons). Built-in graders
include `file`, `tool_calls`, `action_sequence`, `json_schema`, `diff`, `program`
(inline-script), `rubric`/`acquired_rubric` (model-judged), `behavior`,
`skill_invocation_grader.go` (verifies orchestration/skill-call sequences), and
`tool_constraint_grader.go`. The registry/federated-package model (§2.2) lets third
parties publish graders as `github.com/org/repo#grader@vX.Y.Z` refs resolved via
`waza get`/`waza registry add` — a plugin mechanism TASC could target directly rather than
re-implementing its own grader plugin system.

### 2.7 MCP server surface (`waza serve`, tool definitions)

`microsoft/waza:internal/mcp/tools.go:12-122` enumerates the MCP tools Waza exposes:
`waza_eval_list`, `waza_eval_get`, `waza_eval_validate`, `waza_eval_run`,
`waza_task_list`, `waza_run_status`, `waza_run_cancel`, `waza_results_summary`,
`waza_results_runs`, and **`waza_skill_check`** ("Check if a skill is ready for
submission (compliance, tokens, links, eval)"). This is a ready-made integration surface:
TASC could call Waza's MCP server (stdio or TCP JSON-RPC, per
`microsoft/waza:cmd/waza/cmd_serve.go`) from a Rust/CLI process without re-implementing
grading logic, provided TASC treats the Waza binary as an external, versioned tool
invocation rather than a linked library (keeping with "Waza is required" at the
process-integration level, not a vendored dependency).

### 2.8 Semantic/LLM-judged requirement coverage

`microsoft/waza:internal/specverify/semantic.go:11-74` shows `EngineSemanticMatcher`,
which uses a configured judge model to decide whether an eval task exercises a specific
`SKILL.md` requirement when deterministic matching is inconclusive — the prompt template
(`semanticPrompt`, lines 52-74) is a useful reference for TASC if it ever needs an
LLM-judge step to decide "does this Evaluation task actually cover this Insight-worthy
requirement," while keeping the JSON response contract tight (`{"covered":bool,"reason":
string}`) for determinism in automation.

### 2.9 Caveats and honesty notes already baked into Waza (useful language to reuse)

Waza's own docs are unusually explicit about what its signals do **not** prove — useful
precedent for TASC's "Insight" wording (`CONTEXT.md`: *avoid Metric, report*):
- "A graded result is not release authority" — preflight ≠ execution ≠ live-agent
  observation ≠ release approval (`concepts.mdx`).
- "Fixture isolation... is not host filesystem/network sandboxing."
- "`expected.outcomes` is descriptive metadata... not executable state verification."
- Version-boundary callouts distinguish the published stable binary (`v0.38.10`) from
  upcoming/main-only features (assurance profiles, release policies, dashboard Lab) —
  TASC should mirror this discipline given its own roadmap (#13, #14, #15) spans
  not-yet-built capability.

---

## 3. Kratos Agent — authoritative source and identity disambiguation

**Chosen repository:** `LizardLiang/kratos` — https://github.com/LizardLiang/kratos
(default branch `master`; snapshot commit `01220066cda9f7c0ac85cc2e4717183ed2d55bba`,
2026-10-12). Description: *"The God of War - Multi-agent orchestrator plugin for Claude
Code (features, research, project insights)"*. MIT license
(`LizardLiang/kratos:LICENSE`).

**Why this repo and not another "Kratos":** a GitHub search for `kratos` and `kratos
agent` returns many unrelated hits — `go-kratos/kratos` (a Go microservices RPC
framework), `ory/kratos` (identity/authentication server), `aiappsgbb/kratos-agent` and
`Thirumalai-Tech-Developer/Kratos_Agent` (unrelated single-agent coding assistants) — none
of which implement cross-skill/cross-agent **consistency analysis**. `LizardLiang/kratos`
is the only match whose actual subject matter is a structured multi-agent pipeline with
an explicit conflict-detection mechanism, disjoint-vocabulary verdicts, and a
documented hand-off specification between agent roles — i.e., the thing ticket #4 asks
about. Treat this identification as the most defensible reading given available evidence,
but flag it to the requester as an assumption worth a one-line confirmation, since "Kratos
Agent" is not a Microsoft-trademarked or uniquely-named product.

### 3.1 What it is

A Claude Code plugin that orchestrates **19 named "god" subagents** through a fixed
9-stage software-delivery pipeline (PRD → review → decomposition → tech-spec → review →
test-plan → implementation → alignment → code-review/risk). Source layout
(`LizardLiang/kratos` root): `agents/` (god definitions), `pipeline/` (orchestration
procedures), `references/` (shared protocols), `commands/`, `hooks/`, `rules/`,
`templates/`, plus a lockfile-based skill dependency model (`skills-lock.json`).

### 3.2 Pattern 1 — Explicit artifact ownership table (prevents duplicated responsibility)

`LizardLiang/kratos:references/agent-handoff-spec.md:9-23` ("The Pipeline") and
`:25-41` ("Artifact Ownership") define, for every stage, exactly one agent and one
deliverable file, plus a rule: *"If an agent needs one of these files and it is missing,
the agent should stop and tell Kratos to summon the owner."* This is a direct,
transferable pattern for TASC's Consistency Check: a **single-owner-per-artifact/claim**
table is a cheap, deterministic way to detect "duplicated responsibilities" (two Installed
Skills both claiming ownership of the same capability) — TASC could require each Agent
Skill to declare the capability/file/trigger-surface it "owns," then flag any surface
claimed by more than one Installed Skill as a conflict.

### 3.3 Pattern 2 — Disjoint verdict vocabularies (precedence rule that prevents silent overwrite)

`LizardLiang/kratos:references/status-json-schema.md` ("Verdicts" table) assigns each
pipeline stage's reviewer a **distinct verdict vocabulary** — Nemesis:
approved/revisions/rejected; Apollo: sound/concerns/unsound; Hera:
aligned/gaps/misaligned; Hermes: approved/changes-required; Cassandra:
clear/caution/blocked — with the explicit design note: *"Stage 9 vocabularies are
disjoint, so Hermes and Cassandra never overwrite each other."* This is a reusable
**precedence/non-collision rule**: when two analyses run over the same artifact in
parallel, give them non-overlapping output vocabularies/fields so neither can silently
clobber the other's verdict. TASC's Consistency Check result schema (#10) should assign
each check category (conflicts / duplication / incompatible assumptions / terminology
drift) its own named field rather than a single shared "status" value.

### 3.4 Pattern 3 — A real, working conflict detector (closest 1:1 analog to "Consistency Check")

`LizardLiang/kratos:references/status-json-schema.md` § "Conflict detection":
*"`kratos pipeline status` reports `health: conflict` when `prd.md` has a newer file
modification time than `tech-spec.md`. No CLI command writes a PRD version into the spec
stage, so compare `history[]` timestamps when you need the order of events."* This is a
minimal but concrete, machine-checkable incompatible-assumptions detector: **if the
upstream source-of-truth changed after a downstream artifact was produced from it, flag
staleness.** TASC can adapt this directly: if a Skill Source's declared conventions /
terminology change after an Installed Skill was evaluated or watch-listed, flag the
Installed Skill as potentially stale relative to its source — a lightweight, timestamp-only
heuristic that needs no semantic analysis to start adding value.

### 3.5 Pattern 4 — Shared "Agent Protocol" document for terminology/behavioral consistency

`LizardLiang/kratos:references/agent-protocol.md` is injected into every spawned
subagent and defines cross-cutting rules every agent must follow: document selection,
missing-input handling, interactive-question formatting, status-update discipline, a
**Plain Language standard** (modeled on ISO 24495-1: "one term per concept — never rotate
synonyms... define domain terms at first use"), git-safety boundaries, and a fixed
**Output Format** standard (ASCII diagrams for sequences, bullets for unordered facts,
diffs for partial changes). This is a template for **terminology-drift prevention at the
source**: Kratos doesn't just detect drift after the fact — it ships a shared style/vocab
contract that every agent reads, reducing drift at authoring time. TASC's equivalent would
be: publish the `CONTEXT.md` "Language" glossary (Agent Skill, Installed Skill, Skill
Source, Watchlist, Evaluation, Consistency Check, Insight) as a machine-readable glossary
that a Consistency Check can diff Installed Skills' own documentation against, flagging
use of the "Avoid" terms (Plugin, Registry, Test, Lint, Metric, etc.) as drift signals.

### 3.6 Pattern 5 — Structured "gap tree" discovery protocol (unknown-unknowns, not just known checks)

`LizardLiang/kratos:references/discovery-quadrants.md` defines a four-quadrant sweep
(known knowns / known unknowns / unknown knowns / unknown unknowns) with **six concrete
discovery techniques** (premortem, inversion, boundary probe, actor sweep, analogous
failures, checklist escape) and requires a **Discovery Ledger** artifact that travels with
the deliverable. This generalizes beyond checklist-based review: a pure "does skill A's
frontmatter conflict with skill B's frontmatter" checklist only catches *known* conflict
shapes. The quadrant-sweep discipline — forcing an explicit "if I started writing right
now, I would just assume ___" exercise, and an inversion pass on load-bearing assumptions
— is a good design seed for TASC's "incompatible assumptions" detection category, which is
inherently about things neither skill author stated explicitly.

### 3.7 Pattern 6 — Scope-fence fields on every delegated unit of work (remediation boundaries)

`LizardLiang/kratos:references/orchestrator-protocol.md` § "Spawn Prompt Fields" requires
every work-producing spawn to carry `NON-GOALS:` (explicit scope fence lifted from the
PRD) and `STOP-CONDITIONS:` (named early-return triggers: missing prerequisite → report
to the owning upstream stage; genuine ambiguity → `<AGENT> NEEDS CLARIFICATION`). This is
a reusable remediation-output pattern: a TASC Insight that recommends a fix should
likewise carry an explicit non-goal/stop-condition framing so remediation suggestions
don't silently expand scope — directly useful input for #11 ("Define TASC's Insight and
remediation model").

### 3.8 Pattern 7 — Skill lockfile for provenance (parallel to TASC's "Skill Source")

`LizardLiang/kratos:skills-lock.json` records, per adopted skill, `source` (GitHub
repo), `sourceType`, `skillPath`, and a `computedHash` — e.g. `luban` sourced from
`LearnPrompt/luban-skill` at `skills/luban/SKILL.md` with a SHA-256 content hash. This is
essentially a working instance of TASC's own `Skill Source` + provenance/update-detection
concept (`CONTEXT.md`), implemented as a flat JSON lockfile keyed by skill name. Worth
studying as a minimal schema shape (name → source, sourceType, path, hash) before TASC
designs its own canonical local state (#7).

### 3.9 What NOT to take as a runtime dependency

Kratos ships Claude-Code-specific plugin wiring (`.claude-plugin/`, `hooks/path-inject.cjs`
substituting `<KRATOS_ROOT>` tokens into spawned-agent prompts, a bespoke `kratos-bin` CLI
for `pipeline get/update/set-pending`), none of which is portable or meant to be imported.
Per the ticket's explicit constraint, TASC should extract only the **documented
protocols/patterns** above (ownership tables, disjoint vocabularies, timestamp-based
conflict rule, shared terminology contract, quadrant sweep, scope-fence fields, lockfile
shape) and reimplement them natively — Kratos is a reference, not a library or service
dependency, and has no published package/API surface intended for external consumption.

---

## 4. Cross-reference: where Waza and Kratos patterns meet

| TASC concern | Waza gives you | Kratos gives you |
|---|---|---|
| Per-skill **Evaluation** (#9) | Full eval engine: suites/tasks/trials/attempts, graders, MCP server, CI gate exit codes, semantic judge fallback | — (out of scope; single-agent pipeline, not an eval harness) |
| Skill frontmatter quality gate | `waza dev`/`waza check` compliance scoring (trigger/anti-trigger/token-budget rubric) | — |
| Trigger-phrase overlap across skills | `trigger_tests.yaml` + trigger runner (single-skill scoped) as the mechanism to adapt for multi-skill runs | Shared terminology contract as the authoring-time complement |
| **Consistency Check** (#10): duplicated responsibility | `required_skills` preflight dependency declaration | Single-owner artifact table (agent-handoff-spec.md) |
| Consistency Check: precedence / non-collision | Disjoint grader `Kind()`s, independent `GraderResults` | Disjoint verdict vocabularies (status-json-schema.md) |
| Consistency Check: incompatible assumptions / staleness | `waza spec verify` (eval ↔ SKILL.md requirement coverage) | Timestamp-based `health: conflict` rule |
| Consistency Check: terminology drift | — | Plain-Language / one-term-per-concept protocol (agent-protocol.md) |
| **Insight**/remediation model (#11) | `waza suggest` (dry-run/apply eval suggestions) as an existing "propose, then apply" UX precedent | Scope-fence (`NON-GOALS`/`STOP-CONDITIONS`) framing for safe remediation |
| Canonical local state / provenance (#7) | `waza.lock` (resolved remote grader refs) | `skills-lock.json` (source, path, content hash) |

---

## 5. Practical recommendations for TASC

1. **Adopt Waza as an external, versioned CLI/MCP dependency, not a vendored library.**
   Shell out to `waza run`/`waza check`/`waza spec verify`, or talk JSON-RPC to
   `waza serve --tcp`/stdio, from TASC's Rust CLI. This satisfies "Waza is required" while
   keeping TASC's own binary free of Go/Node build requirements (Waza's own build needs
   Go 1.26+, Make, Node.js/npm, and Git LFS for embedded Copilot CLI assets — heavy to
   vendor).
2. **Reuse Waza's compliance-scoring vocabulary (`Low/Medium/Medium-High/High` adherence
   levels, trigger/anti-trigger presence, token budgets) as the baseline for TASC's own
   per-skill Evaluation scoring**, rather than inventing a parallel rubric — this keeps
   TASC's Evaluation results interpretable against upstream Waza output and reduces
   duplicate maintenance of the same frontmatter-quality heuristics.
3. **Build the Consistency Check (#10) as three independent, additive layers** mirroring
   Kratos's design discipline:
   - a static **ownership/overlap table** (which Installed Skill claims which
     trigger phrase / capability — flag duplicates, borrowing Kratos's
     single-owner-per-artifact model),
   - a **staleness/incompatible-assumption** timestamp or version check (Skill Source
     changed after an Installed Skill's last Evaluation — borrowing Kratos's
     modification-time conflict rule), and
   - a **terminology-drift linter** against TASC's own `CONTEXT.md` glossary (borrowing
     Kratos's Plain-Language "one term per concept" protocol) — each layer should emit to
     its own named result field (Kratos's disjoint-vocabulary lesson) so later layers
     cannot silently overwrite earlier findings.
4. **Do not import Kratos's trigger-overlap concept generically** — instead, have TASC
   run the same prompt corpus through Waza's existing per-skill `trigger_tests.yaml`
   mechanism for each Installed Skill, then compare the results. This gets TASC real
   trigger-collision detection while keeping Waza unmodified and avoiding duplicate
   prompt-classification plumbing.
5. **Model remediation output (#11) on Kratos's scope-fence fields**: every TASC Insight
   should carry an explicit non-goal statement and a stop-condition, so an automated or
   semi-automated fix proposal has a bounded blast radius — this is a design discipline,
   not code, so it costs nothing to adopt.
6. **Borrow the lockfile shape, not the file**: TASC's own canonical local state (#7)
   can reuse the `{name → {source, sourceType, path, contentHash}}` shape seen in both
   `skills-lock.json` and Waza's `waza.lock`-style remote-ref pinning, giving provenance
   and update-detection without any dependency on either project's lockfile format.
7. **Re-verify before implementation**: both repositories are active, fast-moving projects
   (Waza had commits as recent as 2026-10-16; Kratos as recent as 2026-10-12) and Waza
   explicitly separates its stable binary (`v0.38.10`) from upcoming/main-only features —
   confirm against `site/src/content/docs/reference/releases.mdx` which capabilities are
   in the version TASC actually shells out to before building integration code against
   them.

---

## 6. Gaps, uncertainties, and suggested follow-ups

- **Identity of "Kratos Agent" is an inference, not a confirmed assertion.** No
  Microsoft-sanctioned or uniquely-named "Kratos Agent" project was found; `LizardLiang/
  kratos` is the best content match among ~15 candidates reviewed, but the requester
  should confirm this is the intended reference before #10 cites it authoritatively.
- **`gh` API/GraphQL access to `microsoft/waza` is blocked by org SAML SSO** in this
  environment (`gh repo view`, `gh api repos/microsoft/waza/topics` both failed with
  "Resource protected by organization SAML enforcement"). All Waza citations in this
  document were obtained via unauthenticated `raw.githubusercontent.com` fetches and a
  public anonymous `git clone` (which still succeeded, including Git LFS content), so the
  content is verified, but repo metadata like topics/stars/issue counts could not be
  pulled through the API from this sandbox.
- **No evidence was found of an official Microsoft doc page (learn.microsoft.com) for
  Waza** beyond the azd-extension-overview page referenced from Waza's own README; Waza's
  authoritative docs live on `microsoft.github.io/waza` (Astro/Starlight site built from
  `site/src/content/docs/*.mdx` in the same repo), not on Microsoft Learn.
- **Not independently verified:** Waza's exact behavior under `waza serve --tcp` (JSON-RPC
  wire format) and `waza assure --calibrate` were read from docs/comments only, not
  exercised by running the binary (no Go toolchain / Git LFS build attempted in this
  research pass). A future ticket implementing the Evaluation contract (#9) should spike
  an actual `waza run`/`waza serve` invocation to confirm.
- **Follow-up search suggestion:** if the requester confirms a different "Kratos Agent"
  is intended (e.g., a specific internal/Microsoft project not indexed by public GitHub
  search), re-run `gh search code "Kratos Agent" --owner <confirmed-org>` and `gh search
  repos kratos --owner <confirmed-org>` to re-scope this section.

---

## 7. Source list (all URLs cited above, consolidated)

- https://github.com/microsoft/waza
- https://microsoft.github.io/waza/
- https://github.com/microsoft/waza/blob/main/site/src/content/docs/concepts.mdx
- https://github.com/microsoft/waza/blob/main/site/src/content/docs/reference/schema.mdx
- https://github.com/microsoft/waza/blob/main/site/src/content/docs/reference/cli.mdx
- https://github.com/microsoft/waza/blob/main/site/src/content/docs/reference/releases.mdx
- https://github.com/microsoft/waza/blob/main/site/src/content/docs/guides/graders.mdx
- https://github.com/microsoft/waza/blob/main/site/src/content/docs/guides/spec-verify.mdx
- https://github.com/microsoft/waza/blob/main/skills/waza/SKILL.md
- https://github.com/microsoft/waza/blob/main/docs/research/waza-eval-registry-design.md
- https://github.com/microsoft/waza/blob/main/examples/required-skills-demo/README.md
- https://github.com/microsoft/waza/blob/main/internal/checks/compliance_score.go
- https://github.com/microsoft/waza/blob/main/internal/graders/grader.go
- https://github.com/microsoft/waza/blob/main/internal/mcp/tools.go
- https://github.com/microsoft/waza/blob/main/cmd/waza/cmd_serve.go
- https://github.com/microsoft/waza/blob/main/internal/specverify/semantic.go
- https://github.com/microsoft/waza/blob/main/internal/trigger/runner.go
- https://github.com/microsoft/waza/releases
- https://github.com/LizardLiang/kratos
- https://github.com/LizardLiang/kratos/blob/master/references/agent-handoff-spec.md
- https://github.com/LizardLiang/kratos/blob/master/references/agent-protocol.md
- https://github.com/LizardLiang/kratos/blob/master/references/orchestrator-protocol.md
- https://github.com/LizardLiang/kratos/blob/master/references/discovery-quadrants.md
- https://github.com/LizardLiang/kratos/blob/master/references/status-json-schema.md
- https://github.com/LizardLiang/kratos/blob/master/skills-lock.json
- https://github.com/LizardLiang/kratos/blob/master/agents/nemesis.md
- https://github.com/LizardLiang/kratos/blob/master/agents/themis.md
- https://github.com/LizardLiang/kratos/blob/master/agents/cassandra.md
- https://github.com/LizardLiang/kratos/blob/master/pipeline/gap-analysis.md
- https://github.com/LizardLiang/kratos/blob/master/pipeline/hephaestus-gate.md

*Snapshots used: `microsoft/waza` @ `d457425953e609d60c4b5059d9f1117d0ac8494c`
(2026-10-16); `LizardLiang/kratos` @ `01220066cda9f7c0ac85cc2e4717183ed2d55bba`
(2026-10-12). Line numbers cited reflect these commits and may shift on later commits.*
