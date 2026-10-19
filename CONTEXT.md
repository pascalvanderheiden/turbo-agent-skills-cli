# TASC

TASC helps people maintain, assess, and improve reusable agent skills across their local coding-assistant environment.

## Language

**Agent Skill**:
A reusable capability bundle that supplies instructions and supporting resources to a coding assistant.
_Avoid_: Plugin, prompt pack

**TASC Command Skill**:
A first-party Agent Skill distributed with each TASC release and installable into explicitly selected supported Adapter Targets at user or project scope. It delegates user-facing TASC operations to the installed `tasc` executable through stable noun-first commands and versioned machine-readable envelopes; it never reimplements domain or persistence logic. A capability handshake gates compatibility, and the binary remains authoritative for Profiles, policies, security checks, plans, approvals, and Local State.
_Avoid_: Wrapper, magic proxy, alternate implementation

**Command Skill Installation**:
A managed Installation of the TASC Command Skill into one Adapter Target and scope. It records first-party provenance and version, tracks successfully approved installs automatically, supports explicit update, repair, and uninstall, and refuses silent overwrite when the user has modified the installed content.
_Avoid_: Automatic enablement, unmanaged helper

**Command Skill Parity**:
Equivalent authorized outcomes across the CLI, dedicated TASC agent, and TASC Command Skill for stable user-facing operations. Parity compares normalized machine-readable results, diagnostics, policy decisions, and approval boundaries; internal diagnostics are not part of the external parity surface.
_Avoid_: Identical interface, duplicated implementation

**Skill Authoring**:
The guided creation of a new Agent Skill from stated goals and constraints, producing a Skill Draft that must satisfy the Agent Skills standard before it is ready for optional follow-up actions.
_Avoid_: Installation, adoption

**Skill Draft**:
The resumable workspace artifact produced during Skill Authoring. An incomplete Skill Draft retains validation findings for correction; a ready Skill Draft has passed deterministic validation but is not installed, evaluated, exported, shared, or published automatically.
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

**Search**:
A strict lookup of Agent Skills using explicit names, keywords, or metadata fields, returning reproducible matches without interpreting an unstated user intent.
_Avoid_: Discover, recommendation

**Discover**:
An intent-oriented way to find Agent Skills across eligible local and remote Skill Sources, using explainable metadata, category, and capability evidence rather than strict keyword matching alone.
_Avoid_: Search, opaque recommendation

**Source Revision**:
The reproducible upstream state observed for a Skill Source, identified by an immutable commit or digest when available, with a content digest as fallback. A declared version is descriptive metadata, not the sole update key.
_Avoid_: Version, timestamp

**Installation**:
A record connecting a Skill Identity to an assistant adapter and local target, including lifecycle state and observed local changes.
_Avoid_: Skill, deployment

**Inventory Projection**:
A human- and machine-readable view of one or more Skill Identity installations, watched interests, and their evidence. It exposes stable action identifiers separately from display row numbers, and keeps lifecycle, tracking, and watch state distinct.
_Avoid_: Skill list, registry

**Provenance Evidence**:
The source evidence associated with an Installation or watched interest, normalized to GitHub repository, marketplace/plugin, local workspace, bundled/first-party, or unknown, with confidence recorded as verified, probable, inferred, or unavailable.
_Avoid_: Marketplace flag, origin guess

**Untracked Installation**:
An Agent Skill installation discovered through an adapter but not yet represented by an Installation in Local State.
_Avoid_: Unknown skill, unmanaged file

**Adoption**:
The explicit act of recording an Untracked Installation in Local State without reinstalling or modifying its files.
_Avoid_: Install, import

**Tracking**:
The user-facing workflow for adopting an already installed skill into Local State so lifecycle, Evaluation, Consistency, Security, and update operations can address it. Tracking is independent per Adapter Target and TASC Profile. Uninstalled skills are watched, not tracked.
_Avoid_: Watchlist, installation

**Watchlist**:
A user-selected set of uninstalled Skill Sources or skill-relative paths monitored for relevant changes. Installing an exactly watched skill ends that skill's Watchlist membership; a broader source-level Watchlist remains and excludes installed matches. A Watchlist never authorizes installation or update.
_Avoid_: Favorites, subscriptions

**Local State**:
TASC's versioned, user-owned metadata about Skill Identities, sources, installations, watchlists, and run references. Evaluation and check outputs are stored as separate local artifacts; credentials are not copied into Local State.
_Avoid_: Cache, registry

**TASC Profile**:
A named user or project scope over Local State that groups tracked skills, watchlists, status, policies, and related run references without owning installed or authored skill content. Project operations select the project Profile when one exists and otherwise fall back to the user Profile; the two Profiles are not merged and records are not migrated automatically.
_Avoid_: Skill Workspace, installation target

**Slash Command**:
A deterministic command addressed to the TASC agent using noun-first syntax, with compatibility aliases normalized to one canonical command. Slash Commands and natural-language requests share the same authorized TASC operations.
_Avoid_: Prompt shortcut, alternate API

**Command Palette**:
The live, filterable entry surface for discovering and choosing canonical Slash Commands in an Agent Session.
_Avoid_: Command menu, prompt shortcut

**Picker**:
A bounded, navigable selection surface for choosing finite command targets such as models, skills, profiles, evaluations, or bundles.
_Avoid_: List, selector dialog

**Presentation Event**:
A human-readable terminal category that distinguishes input, TASC output, prompts, information, work in progress, warnings, errors, completed outcomes, and next-step guidance without changing the underlying command result.
_Avoid_: Log line, diagnostic code

**Agent Session**:
One interactive TASC agent conversation with its selected Profile, model preference, status, pending approvals, progress state, and run references. Session preferences may override Profile preferences without changing them unless explicitly saved.
_Avoid_: Profile, Run Artifact

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

**Security Policy**:
A TASC Profile's rules for deciding whether Security Findings and incomplete required checks allow, require approval for, or deny a planned source mutation.
_Avoid_: Security Check, suppression

**Security Override**:
An explicit, reasoned, auditable authorization to proceed with one planned mutation when required Security Checks are unavailable, failed, or inconclusive. It does not suppress findings or change Security Policy.
_Avoid_: Suppression, ordinary approval

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

**Audit**:
A read-only deterministic health assessment of the selected TASC Profile's Local State, installations, sources, targets, and existing evidence. Audit findings are classified as errors, warnings, or optimizations and do not refresh or mutate the evidence they reference.
_Avoid_: Evaluation, repair

**Statistics Projection**:
A read-only, profile-scoped summary of TASC inventory, provenance, lifecycle, watch and tracking state, audit finding classes, and comparable artifact outcomes. Statistics describe observed state without changing it or collapsing stale evidence into current evidence.
_Avoid_: Audit, telemetry

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

**Bundle Deployment**:
The explicit installation of a Skill Bundle into selected Adapter Targets after resolving its source constraints, checking security policy, reviewing one bounded plan, and preserving user-owned changes.
_Avoid_: Bulk install, silent sync

**Profile Skill Source**:
An optional Skill Source repository linked to a TASC Profile as the source-controlled home for authored Agent Skills, Categories, and Skill Bundles.
_Avoid_: Marketplace, credential store

**Watch Check**:
A user-triggered comparison between a Watchlist target and its last observed Source Revision that reports changes without updating an Installation.
_Avoid_: Sync, watcher

**Adapter Target**:
The coding assistant and local destination associated with an Installation.
_Avoid_: Platform, deployment target

**Documentation Surface**:
One of TASC's user-facing documentation entry points: the concise README, the lean landing page, or a versioned detailed guide. The README is the canonical source for the landing page's selected common-use-case content.
_Avoid_: Duplicate manual copy, API reference only

**Release Artifact**:
An immutable, versioned distribution output for a supported operating-system and architecture target, accompanied by checksums, software-bill-of-materials data, and build provenance evidence.
_Avoid_: Unverified binary, mutable download

**Public Delivery**:
The user-visible path from an activated public repository through verified Release Artifacts, the curl-style installer, and GitHub Pages Documentation Surfaces. Public Delivery is unavailable until the repository settings and release governance prerequisites are enabled.
_Avoid_: Source checkout, private preview

**Vertical Slice**:
A dependency-ordered increment that delivers one usable TASC capability across its domain services, CLI, dedicated agent, machine-readable contract, and relevant tests before the next increment begins.
_Avoid_: Backend-only layer, demo slice

**Acceptance Gate**:
A measurable condition that must pass before a TASC roadmap increment or release is considered ready, including deterministic behavior, versioned JSON compatibility, surface parity, and applicable interactive/non-interactive rendering checks.
_Avoid_: Manual confidence, informal sign-off
