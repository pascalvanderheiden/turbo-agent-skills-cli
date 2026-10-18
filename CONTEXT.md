# TASC

TASC helps people maintain, assess, and improve reusable agent skills across their local coding-assistant environment.

## Language

**Agent Skill**:
A reusable capability bundle that supplies instructions and supporting resources to a coding assistant.
_Avoid_: Plugin, prompt pack

**TASC Command Skill**:
A first-party Agent Skill distributed with TASC and installable into supported Adapter Targets. It enables a user’s own coding assistant to perform the same authorized TASC operations as the CLI and dedicated TASC agent while preserving TASC’s plans, approvals, policies, and Local State.
_Avoid_: Wrapper, magic proxy, alternate implementation

**Skill Authoring**:
The guided creation of a new Agent Skill from stated goals and constraints, producing skill files that are validated before optional installation or sharing.
_Avoid_: Installation, adoption

**Skill Draft**:
The workspace files produced during Skill Authoring before deterministic validation succeeds. A Skill Draft may be retained for correction but is not presented as a ready Agent Skill.
_Avoid_: Installed Skill, validated skill

**Skill Workspace**:
An explicitly selected writable local source used to author or revise an Agent Skill. A Skill Workspace is distinct from an Installation and is the only place TASC may apply skill-content edits.
_Avoid_: Installation target, installed copy

**Skill Identity**:
The stable, assistant-neutral identity of an Agent Skill, derived from its normalized upstream origin and skill-relative path. A Skill Identity is distinct from any particular source revision or local installation.
_Avoid_: Installation, version

**Installed Skill**:
An Agent Skill made available through a recorded installation target for a coding assistant.
_Avoid_: Enabled plugin, Skill Identity

**Skill Source**:
The origin from which an Agent Skill can be discovered, installed, shared, or updated.
_Avoid_: Registry, marketplace

**Source Revision**:
The reproducible upstream state observed for a Skill Source, identified by an immutable commit or digest when available, with a content digest as fallback. A declared version is descriptive metadata, not the sole update key.
_Avoid_: Version, timestamp

**Installation**:
A record connecting a Skill Identity to an assistant adapter and local target, including lifecycle state and observed local changes.
_Avoid_: Skill, deployment

**Untracked Installation**:
An Agent Skill installation discovered through an adapter but not yet represented by an Installation in Local State.
_Avoid_: Unknown skill, unmanaged file

**Adoption**:
The explicit act of recording an Untracked Installation in Local State without reinstalling or modifying its files.
_Avoid_: Install, import

**Tracking**:
The user-facing workflow for adopting an already installed skill into Local State so lifecycle, Evaluation, Consistency, Security, and update operations can address it. Uninstalled skills are watched, not tracked.
_Avoid_: Watchlist, installation

**Watchlist**:
A user-selected set of Skill Sources or skill-relative paths monitored for relevant changes. A Watchlist detects changes but does not silently update installations.
_Avoid_: Favorites, subscriptions

**Local State**:
TASC's versioned, user-owned metadata about Skill Identities, sources, installations, watchlists, and run references. Evaluation and check outputs are stored as separate local artifacts; credentials are not copied into Local State.
_Avoid_: Cache, registry

**TASC Profile**:
A named user or project scope over Local State that groups tracked skills, watchlists, status, and related run references without making installed skill content writable.
_Avoid_: Skill Workspace, installation target

**Evaluation**:
A repeatable assessment of an Agent Skill against defined scenarios and quality criteria.
_Avoid_: Test, review

**Evaluation Suite**:
A versioned TASC descriptor that identifies Waza evaluation artifacts, Evaluation Tasks, graders, execution profiles, and policy used to assess an Agent Skill.
_Avoid_: Test plan, benchmark

**Evaluation Run**:
One invocation of an Evaluation Suite against a Skill Identity, Source Revision, and Adapter Target, producing one immutable Run Artifact.
_Avoid_: Test run, execution

**Evaluation Task**:
A named prompt, input, expected observable outcome, and optional follow-up or checkpoint within an Evaluation Suite.
_Avoid_: Test case, example

**Trial**:
One measured repetition of an Evaluation Task.
_Avoid_: Run, attempt

**Attempt**:
One execution within a Trial. TASC records individual Attempt evidence only when the evaluator exposes it; otherwise it records the observed attempt count without inventing history.
_Avoid_: Retry, rerun

**Evaluation Profile**:
A named, versioned policy overlay for an Evaluation Suite that changes execution settings without changing its Evaluation Tasks.
_Avoid_: Environment, mode

**Baseline Run**:
A prior comparable Evaluation Run used to identify changes in outcomes or metrics.
_Avoid_: Source Revision, target

**Control Run**:
An Evaluation Run performed under matched conditions with the Agent Skill disabled, used to measure its observed contribution.
_Avoid_: Baseline, dry run

**Consistency Check**:
An analysis of multiple Agent Skills for conflicts, incompatible assumptions, duplicated responsibilities, and terminology drift.
_Avoid_: Evaluation, lint

**Consistency Finding**:
An evidence-backed compatibility risk or coverage result produced by a Consistency Check for a selected set of Agent Skills.
_Avoid_: Error, verdict

**Security Check**:
A local-first analysis of Agent Skill contents and dependencies for exposed secrets, unsafe instructions or permissions, injection or exfiltration patterns, risky executable content, supply-chain indicators, and suspicious external references.
_Avoid_: Consistency Check, live penetration test

**Security Finding**:
An evidence-backed risk identified by a Security Check, with affected content, severity, confidence, and bounded remediation guidance.
_Avoid_: Exploit, generic warning

**Ownership Claim**:
A scoped declaration that an Agent Skill owns a capability, command or tool surface, file or path, trigger boundary, or non-goal.
_Avoid_: Exclusive lock, responsibility guess

**Precedence Rule**:
A scoped, reviewable rule that orders competing Agent Skill claims for a defined surface.
_Avoid_: Global priority, installation order

**Suppression**:
A versioned, time-bounded local decision to omit a specific Consistency Finding or Insight from default actionable output while retaining its evidence and suppression reason.
_Avoid_: Ignore, delete

**Insight**:
An actionable, user-facing recommendation derived from lifecycle state, Evaluations, or Consistency Checks. An Insight may aggregate closely related evidence, but each contributing finding remains independently traceable.
_Avoid_: Metric, report

**Insight Status**:
The lifecycle state of an Insight: `new`, `acknowledged`, `snoozed`, `suppressed`, `resolved`, `stale`, or `inconclusive`. Status changes do not alter the underlying evidence.
_Avoid_: Finding status

**Remediation Proposal**:
A typed, bounded recommendation for addressing an Insight, including its rationale, prerequisites, affected evidence, risk, reversibility, and expected verification. A proposal does not change an Agent Skill.
_Avoid_: Advice, automatic fix

**Remediation Action**:
An explicitly approved execution of a Remediation Proposal, recorded with its scope, preconditions, actor, tool provenance, result, and follow-up verification.
_Avoid_: Implicit update, silent repair

**Run Artifact**:
A local, immutable record of an Evaluation, Consistency Check, or Watch Check, including the exact inputs and Source Revisions used.
_Avoid_: Log, report

**Portable Skill Bundle**:
An explicit export containing an Agent Skill, its source metadata, and selected Run Artifacts without credentials or unrelated Local State.
_Avoid_: Backup, package

**Skill Category**:
A descriptive tag used to organize and discover Agent Skills without changing installation, tracking, or execution behavior.
_Avoid_: Skill Bundle, capability owner

**Skill Bundle**:
A named, versioned, deployable manifest of selected Skill Identities, source constraints, and installation intent that can be shared without copying credentials or unrelated Local State.
_Avoid_: Skill Category, Portable Skill Bundle

**Watch Check**:
A user-triggered comparison between a Watchlist target and its last observed Source Revision that reports changes without updating an Installation.
_Avoid_: Sync, watcher

**Adapter Target**:
The coding assistant and local destination associated with an Installation.
_Avoid_: Platform, deployment target
