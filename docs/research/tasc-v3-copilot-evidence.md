# Research: Copilot integration and trustworthy local skill evidence

**Ticket:** [#135](https://github.com/pascalvanderheiden/turbo-agent-skills-cli/issues/135).
**Observed:** 2026-10-23. **Scope:** research/spec charting, not implementation,
deployment, a repository Security Check, or final human-in-the-loop decisions.
Competitor and general CLI UX comparison are deliberately excluded.

## Executive findings

1. **An assistant-first, CLI-backed TASC Command Skill is feasible.** Copilot
   CLI and VS Code support Agent Skills and local MCP tools. A skill can delegate
   to a reliable `tasc` executable; neither discovery nor a description guarantees
   invocation. Keep deterministic operations and authorization in the binary,
   not in instructions. [S1–S4]
2. **Quiet, bounded checks can use documented lifecycle hooks.** Current VS Code
   documentation distinguishes its shared **Copilot** harness from the older
   **Local** harness; their hooks/settings/payloads are not interchangeable.
   VS Code's hooks experience is Preview. Hooks are not a continuous filesystem
   observer and do not cover edits made outside their runtime. [S5, S6]
3. **Skill activation evidence exists today:** released Copilot SDK 1.0.26 exposes
   `skill.invoked`, including injected content and path; optional fields identify
   trigger, model and discovery source. CLI/app local session records are
   documented. However, there is no demonstrated stable, universal passive
   subscription to every existing IDE/CLI session, and activation does not prove
   instruction compliance or outcome contribution. [S7–S10]
4. **“Local” does not imply “never uploaded.”** Current GitHub docs say CLI/app
   local sessions sync to the account by default, subject to policy.
   `/chronicle` can send prior prompts/context/responses to a model.
   No-cloud TASC ingestion must be independent of these facilities, and must not
   silently change host settings. [S9–S11]
5. **Effectiveness needs approved experiments, not usage counts.** Repeated,
   matched skill-on/skill-off Trials can estimate contribution in a bounded
   Evaluation. Waza's current strict native Copilot comparison profile explicitly
   excludes skills; it is not already a turnkey skill-ablation evaluator. [S15,
   S16] No private logs/skills were submitted to model endpoints or evaluated in
   this research.

## Method, currency and status vocabulary

Read the ticket, root `CONTEXT.md`, domain instructions and existing research.
Use the domain meanings of Installation, Adapter Target, Audit, Security Check,
Consistency Check, Evaluation, Control Run and Run Artifact. No relevant ADR
was present in this research checkout.

Context7 was queried for Copilot CLI hooks/logs, SDK events/permissions and
GitHub telemetry/session privacy. Its results were followed back to first-party
docs and pinned source: Context7 itself is discovery, not authority.
Only public documentation identifiers/questions were sent to it.
No installed private sessions, skill contents, credentials or evaluation inputs
were inspected or transmitted.

- **Supported:** explicitly documented for that host/surface; not a promise that
  every installed version supports it.
- **Preview/experimental:** explicitly labelled so by the owner; gate on actual
  installed version and capability.
- **Absent:** explicitly unsupported for that surface/profile.
- **Unknown/unverified:** not established by inspected sources, or not verified
  against a running installed host. Not equivalent to absent.
- **Proposal:** research-derived option/constraint, not a shipped TASC feature
  or settled product decision.

Release API observations: CLI latest stable was
[`v1.0.103`](https://github.com/github/copilot-cli/releases/tag/v1.0.103)
(2026-10-22); SDK latest stable was
[`v1.0.26`](https://github.com/github/copilot-sdk/releases/tag/v1.0.26)
(2026-10-22), whose tag resolves to
`f527e6134f9b38980af8aa796b4c959458a88769`.
The released SDK README now says **generally available**, not technical preview.
Individual RPCs can still be experimental. [S7, S10]
Do not carry historical SDK-preview assumptions into v3.

GitHub docs are pinned to `935221fd662811fa04955b1759bf67b96b7ae028`
(commit dated 2026-10-23). VS Code docs are pinned to
`a03090533810eb02e5b9a2a650e2a3df32192b42` (HEAD resolved 2026-10-23);
rendered hooks/debug pages displayed 2026-10-21. These are documentation
snapshots, **not proof of release availability on a user's installation**.
The feature matrix is itself public preview and some version tables lag the
new harness-specific pages. Prefer actual version/capability probes over an
inferred universal minimum version. [S3, S6, S12]

## Host capability matrix

| Copilot host / runtime | Agent Skills and command seam | Tools / MCP | Hooks and triggers | Local evidence / limitations | Feasibility status |
| --- | --- | --- | --- | --- | --- |
| **Copilot CLI** | Project `.github/skills`, `.claude/skills`, `.agents/skills`; personal `~/.copilot/skills`, `~/.agents/skills`. Relevance-based selection or explicit `/skill-name`; `/skills info/reload` and terminal `copilot skill` management documented. | Local stdio and remote MCP; explicit tool subsets, trust/policy apply. Shell can invoke TASC but shell approval is not operation-level TASC approval. | Repository/user/settings/plugin hooks; start/end, prompt, tool success/failure, stop and subagent events documented. Load changes at startup. No documented filesystem-change hook in inspected reference. | `session-state/<id>/events.jsonl`, workspace artifacts, diagnostic `logs/`, cross-session SQLite store. Version-bound event ingestion feasible; complete passive invocation/cost coverage unverified. | **Supported seams**; ingestion stability/coverage requires adapter tests. [S1, S4, S5, S8] |
| **VS Code, Copilot harness / Agent Host** | Skills supported, project/personal locations documented; explicit slash invocation and relevance loading. Native locations preferred over editor-only custom search settings. | Shared Copilot runtime plus editor MCP configuration/forwarding rules; trust, availability and approvals are separate. | Shared Copilot hook format; editor hooks UX **Preview**. Config belongs to execution host in remote/container sessions. A VS Code extension can additionally use stable filesystem watcher APIs while active. | Agent Debug Logs **Preview**, capture setting **experimental**, not retroactive; local persisted logs and user-exportable OTLP JSON. No stable all-session extension subscription established here. | Skills/MCP **supported**; debug/hook UI maturity and installed harness must be recorded. [S3, S6, S13, S14, S17] |
| **VS Code, older Local harness** | Its own customization behavior/settings; current docs restrict prompt files to Local. A prompt file is not the portable TASC Command Skill. | Editor tools/MCP available; do not assume shared-runtime tool identifiers. | Separate Local hook reference and enablement settings; aliases do not guarantee compatible payloads. | Separate experimental debug-file logging setting; export/diagnostics preferable to guessing internal database formats. | Existing surface **documented**, migration/version-specific; do not silently convert. [S6, S13] |
| **GitHub Copilot app, local sessions** | Repository/CLI skills automatically available; Customize UI manages skills/plugins. | Repository/CLI MCP configurations reused. | VS Code docs identify shared Copilot harness/format across app/CLI/editor; app-specific admission and exact event delivery still need a smoke test. | GitHub explicitly documents complete local records under `~/.copilot/session-state/`; account sync separate and default-on subject to policy. No public promise that arbitrary app-private SQLite schemas are a TASC API. | Skill/MCP/local storage **supported**; exact app runtime/build and evidence parity **unverified**. [S6, S9, S18] |
| **Copilot SDK-driven session** | `skillDirectories`, `disabledSkills`; register custom tools around binary operations. Not the existing assistant's UI. | Local/remote MCP, available/excluded tool filters, permission callbacks. | Programmatic hooks and `session.on` events. SDK callback behavior can differ from command-hook behavior. | `skill.invoked`, tool lifecycle, session history and ephemeral usage available for sessions the integration legitimately controls/connects to. Starting/resuming is not a passive global observer. | SDK **GA**, selected RPCs **experimental**. Suitable for approved Evaluations, not default replacement assistant. [S7, S10] |
| **Copilot cloud agent / GitHub.com** | Skills supported in repository; no assumption of access to personal local skill dirs. Code review also lists skills, but not equivalent tool execution. | Cloud-agent MCP supported; tools/environment differ from local. Code-review shell/MCP execution parity not established here. | Cloud agent has repository hooks, a subset of events, ephemeral Linux filesystem, constrained network and non-interactive pre-granted tools. | Session log on GitHub, visible to repository readers by default; not a local-only ingestion source. | Cloud-agent capabilities **supported**, but **outside initial no-cloud evidence boundary**. Code-review parity **unknown**. [S2, S5, S9] |
| **Visual Studio / JetBrains** | Matrix: Visual Studio skills supported (18.6.0 row); JetBrains skills **Preview** (1.5.64–1.5.66 rows). | MCP supported. | No equivalent documented hook contract established in this investigation. | Stable invocation-log/export adapters not established. | Potential future Copilot targets; no initial parity claim. [S12] |
| **Eclipse / Xcode / NeoVim** | Matrix marks Agent Skills **absent**; agent/chat availability does not imply skills support. | MCP supported in Eclipse/Xcode, absent in NeoVim matrix. | Equivalent hooks unknown. | Equivalent local evidence unknown. | CLI fallback only; do not claim Command Skill support. [S12] |

The matrix is about Copilot, not a comparison of competing assistants.
“Supported MCP” does not mean a local executable exists on a remote/container
host; it must be installed there. [S14]

### Important configuration and permission differences

- CLI user settings now use `~/.copilot/settings.json`; `config.json` is managed
  internal state. Current hook docs explicitly say user `config.json` is no
  longer read for hooks. Context7's changelog-derived historical hints must not
  override this current reference. [S5, S8]
- CLI does **not** read `.vscode/mcp.json` directly. It reads `.mcp.json`,
  `.github/mcp.json` and user `mcp-config.json`. Current VS Code can forward
  compatible configurations, but VS Code-format files have forwarding
  restrictions. One shared pathname is not proof of shared interpretation.
  Project servers require appropriate workspace trust. [S4, S14]
- CLI `allowed-tools: shell`/`bash` in a skill can remove shell confirmations;
  GitHub explicitly warns of arbitrary command execution. Do not ship a
  convenience TASC Command Skill that broadly grants this. In SDK sessions the
  permission handler still decides despite `allowedTools` in activation
  evidence. Neither a pre-approved tool nor “agent asked” substitutes for a
  bounded approved TASC mutation/evaluation plan. [S1, S7]
- Current shared-runtime command hooks can use `exec` plus an argument array,
  avoiding shell interpolation. Validate event JSON as untrusted input; never
  interpolate tool arguments into a command string. Hook installation itself
  changes executable configuration and needs explicit approval. [S5, S6]
- `preToolUse` command errors deny tool calls, but **timeouts fall through to
  normal permissions, even policy-hook timeouts**. HTTP pre-tool errors also
  fall through. Return `{}` for observation without granting permission;
  `postToolUse` cannot undo an executed action. Do not use a best-effort hook as
  the sole authorization boundary. [S5, S6]

## Practical integration options, triggers and fallbacks

These are candidate designs to take to product decisions, not installation
instructions or authorization to enable them.

| Option | Useful trigger / behavior | Coverage limits | Safe fallback |
| --- | --- | --- | --- |
| **TASC Command Skill → installed CLI** | Explicit user skill invocation; model relevance selection can suggest Audit/status/next action. Stable noun-first commands and versioned machine-readable envelopes; capability handshake before dispatch, per `CONTEXT.md`. | Model may skip, paraphrase or misroute instructions. Skill is not a reliable scheduler. Shell arguments/results can enter model context. | User directly invokes the same CLI operation; binary policy remains authoritative. Return a minimal status, not raw private content. |
| **Local stdio TASC MCP tool → CLI/domain boundary** | Explicit/model-selected bounded read tools, exact Installation/Profile identifiers. Structured schemas can reduce shell-string ambiguity. | Server discovery/start does not guarantee call selection or observation of unrelated tools. Tool results enter assistant context; launching stdio runs code. | Command Skill/CLI; disabled or disallowed MCP is a declared unavailable capability, not an error requiring policy bypass. [S4, S14] |
| **Opt-in command hooks → deterministic CLI checks** | On `sessionStart`, compare scoped fingerprints; after relevant successful edit/write tools, recheck changed skill/config paths; `agentStop` can coalesce a final bounded check. | `postToolUse` covers successful calls, not failed/partial writes; shell can mutate many paths. Need failure/end rescan. No coverage while runtime is stopped or for unrelated external editors. | On next explicit check/start, rescan allowed paths and label intervening coverage gaps. No daemon assumption. [S5, S6] |
| **Small optional VS Code extension → CLI** | `createFileSystemWatcher` create/change/delete events for selected skill/config roots; debounce, fingerprint and run deterministic checks while extension is active. | Separate implementation/distribution; remote extension-host location, exclusions, watcher loss/coalescing and symlink/rename behavior need validation. This is a file trigger, not evidence the agent invoked a skill. | Full fingerprint comparison at explicit run/start; no need to implement an alternative chat assistant. Stable watcher API exists. [S17] |
| **Opt-in local import → evidence projection** | User-selected session files/exports and time/repository bounds; explicit incremental import, or bounded import at an approved lifecycle trigger. | Historical logs can be missing, truncated, migrated, private or not attributable to an Installation. File existence is not consent. | Inventory/checks still work; usage shown as unavailable or partial. [S8, S9, S13] |
| **SDK → approved Evaluation** | Explicit Evaluation Run with isolated input, exact tools/permissions, model and budget; capture structured events. | Creates/controls a model session; not continuous observation of the existing assistant. Runtime/API support must be verified. | Report unsupported/inconclusive; deterministic preflight may still run without launching a model. [S7, S10] |

### Trigger filtering proposal

Relevant changes include the selected Installation's `SKILL.md`, referenced
scripts/resources, discovery/enablement/configuration, tool/permission policy,
and the selected set's Ownership Claims/Precedence Rules. Ordinary source edits
outside these dependencies should not trigger broad skill analysis.

Use bounded path sets, byte/time limits, content digests and debounce/coalescing;
inspect actual bytes rather than trusting the tool's reported path. Catch
create/delete/rename and failed/partial-write cases through a bounded rescan.
Invalidate affected evidence rather than evaluating everything after every tool.
Do not block an agent turn merely to run an optimization check.

Quiet means no repeated success messages, no raw private content in assistant
context, and deduplicated actionable diagnostics; it must not hide unavailable
coverage. A passive hook should avoid `allow`, prompt rewriting, stop blocking
and model-generated follow-ups. `sessionEnd` is not guaranteed after a crash,
so end-only collection is insufficient. These are proposed operating safeguards,
not claims that hooks guarantee delivery.

## Evidence: what can actually be observed?

| Observation | Defensible statement | Not defensible |
| --- | --- | --- |
| Skill files present; discovery list/info | Available/discovered under this configuration at this time. | Used, enabled in every session, or effective. |
| User typed `/name` or asks “use skill” | Requested invocation. | Confirmed injection, completion or compliance. |
| Explicit `skill.invoked` event | Runtime reports activation/injection; retain trigger (`user-invoked`, `agent-invoked`, `context-load`) when provided. | Every instruction followed, resources executed, or activation caused success. |
| Tool reads `SKILL.md` / scripts | Observed resource access or execution attempt/result, if tool evidence supports it. | Semantic skill activation; later behavior was caused by that file. |
| TASC CLI/MCP invocation receipt | TASC operation requested/completed with recorded policy/result and input identity. | Other skills invoked; TASC enabled the whole task's outcome. |
| Session success/user acceptance beside activation | Co-occurrence/correlation in an observed session subset. | Causal effectiveness, time saved, or counterfactual success without the skill. |
| Repeated matched Control Runs | Estimated measured contribution for the tested tasks/configuration, with uncertainty. | Universal effectiveness, compatibility or safety certification. |

Activation/event semantics come from released SDK event documentation and
generated types. [S7] The distinctions above are research reasoning: no live
effectiveness experiment was performed.

### Local logs: availability is not a stable analytics contract

**CLI/app:** GitHub documents complete local session state and a subset indexed
in SQLite. `events.jsonl` is documented; `logs/process-*.log` is diagnostic
logging, not a clean usage ledger. `session-store.db` is automatically managed;
its tables are not established here as a stable external schema. Do not query
or repair an active database using guessed schema, and never run
`/chronicle reindex` for TASC ingestion: docs say it also syncs data. [S8, S9]

**Structured events:** SDK 1.0.26 `skill.invoked` has required `name`, `path`,
`content`; optional `source`, `trigger`, `model`, `invokedAtTurn`, plugin
identity/version and allowed tools. `path` can be empty for SDK-provided skills.
This is substantially stronger than searching prose for skill names. However,
the types/reference do not prove that every historical CLI/app/IDE version
emits every field, or that an event covers all manually pasted instructions.
The same name may resolve to different project/personal/plugin installations.
Retain path/source resolution and ambiguity; do not join on name alone. [S7]

**Persistence:** event envelopes distinguish persisted from `ephemeral` events.
`assistant.usage` and deltas/progress are ephemeral and are not replayed on
resume. A historical activation record cannot reconstruct all token/cost
totals; “missing” is not zero. Live subscriptions can also miss startup events or
lose buffered events, and generated protocol/RPC evolution needs compatibility
handling. [S7, S10] No inspected source promises a lossless global listener for
all existing independent Copilot sessions.

**VS Code:** Agent Debug Logs persist locally, but their UI is Preview and
capture switches are experimental and harness-specific. Capture must be enabled
before activity; it is not retroactive. User-exported OTLP JSON is a documented
interchange option. Debug views can reveal discovery, model requests, tools,
context and errors, but a guaranteed skill-specific activation event and stable
raw on-disk schema for every editor harness remain unverified. Do not scrape
private editor state as if it were a supported API. [S13]

### Proposed evidence record and missingness

Keep host/version/harness/execution location, parser/schema version, approved
scope, session/event pseudonymous IDs, timestamp, coverage interval, event kind,
skill identity resolution and confidence, plus event-content digest when
available. Distinguish event content as injected from currently installed
content. Record gaps, truncation, unsupported events, excluded sessions and
unknown fields. Deduplicate by host/source/event identity rather than prompt
text; imported events may be replayed or appear in multiple exports.

Report “N observed activations in imported coverage,” not “N uses total.”
Keep `unavailable`, `partial`, `inconclusive`, `stale`, `failed`, `not run` and
`not applicable` distinct. A zero requires a known covered interval and known
event support; absence from an incomplete log proves nothing. Aggregate
session usage belongs to the session, not each skill; splitting its tokens or
cost evenly among activated skills would fabricate attribution.

## Freshness must bind actual installation, contents and environment

Proposal: evidence should have a reproducible **input fingerprint**, not just
“checked recently” or declared skill version:

- **Identity and Installation:** Skill Identity, Adapter Target, user/project
  scope and TASC Profile; resolved path and discovery/precedence/enablement.
  Another Installation of the same Skill Identity does not inherit freshness.
- **Bytes:** normalized source origin and Source Revision when known, plus
  digest of actual installed `SKILL.md`, referenced/packaged resources and
  dependency/lock bytes within a declared boundary. Untracked/local edits matter.
  A Git commit is insufficient if the worktree is dirty.
- **Execution environment:** actual host/build/harness, operating system,
  runtime and TASC/checker/evaluator versions, resolved tool/MCP configuration,
  permissions, hooks, trust/sandbox, and relevant instruction/agent context.
  Record unavailable provider/runtime identities rather than substituting a
  requested model label for an observed model revision.
- **Assessment inputs:** checker/rule/policy versions, selected skill-set digest
  for Consistency, Evaluation Suite/tasks/graders/fixtures/profiles, model and
  relevant execution settings, plus observation time and coverage.

These requirements follow from host-specific discovery and event/environment
differences [S1, S3–S10] and Waza's explicit source-binding/comparison limits
[S15, S16]. They are proposed TASC evidence semantics, not a claim that Copilot
already supplies every field.

Content/config/environment changes invalidate only relevant claims; retain old
Run Artifacts as historical evidence, not current passes. Rechecking hashes is
cheap; refreshing a Security/Consistency/Evaluation result is a separate
operation. An Audit may compare recorded freshness metadata, but by domain
definition does **not** refresh the referenced evidence.

A digest of injected `SKILL.md` proves neither which ancillary resource bytes
were subsequently read nor that an on-disk executable matches the running
image. Mutable URLs, unpinned dependencies and unobserved external state require
explicit incomplete coverage. A time-to-live can bound age but cannot make
changed bytes fresh.

## Keep assessment kinds and approval boundaries separate

| Capability | Bounded deterministic/read-only portion | Limit / approval-required portion |
| --- | --- | --- |
| **Audit** | Assess selected Profile's Local State, Installation paths, provenance completeness, target availability and existing evidence/freshness; errors/warnings/optimizations. | No repair, evidence refresh, installation change or model invocation. Health is not a safety/effectiveness verdict. |
| **Security Check** | Parse skills as data; known-secret patterns, suspicious URLs/permissions, executable/reference inventory, digest/provenance/lock checks with offline rules. | No execution of embedded scripts, remote URL fetching, package installs, exploit testing or model analysis by default. Static findings are risks, not “secure” certificates; unsupported analysis stays incomplete. |
| **Consistency Check** | Structural duplication, exact contradictory declarations, overlapping Ownership Claims, unsupported tool/path references, explicit Precedence Rule checks. | Natural-language semantic conflicts, implicit assumptions and contextual compatibility cannot be completely decided by these rules. Model-assisted analysis requires separate approved disclosure/cost policy; quote evidence/confidence, not certainty. |
| **Evaluation** | Validate descriptors/fixtures, compare existing comparable artifacts; offline mock validates harness only. Approved deterministic graders can check observed outputs. | Launching an agent/model, model graders, generated suites/remediation and third-party executable graders require scoped approval and sandbox/resource policy. Deterministic graders can themselves execute unsafe code. |

Domain definitions come from `CONTEXT.md`; boundaries above are proposals for
the stated user constraints. “Read-only” means no skill/workspace/host-setting
mutations; even a read-only operation may disclose data if its output is sent
to Copilot. Persisting local check artifacts/imported metadata is itself a write
and needs an explicitly defined opt-in storage boundary. A strict read-only
invocation should permit no such persistence. No model-powered check belongs
inside an automatic quiet-check path under the present authorization.

## Privacy, retention and cost constraints

### Separate three data planes

1. **TASC local inventory/checks/usage projection:** opt-in ingestion, local
   storage only, no network exporter or cloud telemetry; no automatic model
   summarization. This can be independent of Copilot's services.
2. **Copilot host's own telemetry/session sync:** not controlled merely because
   TASC is local. GitHub documents `remote: "on"` and `remoteExport: true`
   defaults, applicable policy, and overrides; a local file importer must not
   enable/reindex/query cloud history. `remote: "off"`/`--no-remote` and
   `remoteExport: false` are documented controls, but changing them needs
   approval and actual effective-setting verification. [S8, S9]
3. **Assistant/model context and approved Evaluations:** sending a tool result,
   skill or transcript to the existing assistant is data transmission even
   without a second vendor. Current authorization covers **no private logs or
   private skills sent to external model endpoints**. A future model-based
   feature needs separate consent specifying content, recipient, purpose,
   retention and spend—not inferred from local-ingestion consent.

GitHub's OTel export is off by default, but an endpoint, enablement variable or
file-exporter path can activate it. A local JSON-lines file exporter is a possible
opt-in alternative to transcript ingestion; content capture defaults off. It
does not establish skill-attribution completeness. Enterprise settings can also
configure telemetry. `--disable-client-telemetry` explicitly does **not** disable
server-side telemetry. Therefore TASC cannot honestly promise “Copilot emits no
telemetry/cloud data” merely by setting a flag. [S8, S11]

If “NO cloud telemetry” means no TASC usage exporter, it is implementable. If it
means no host/service telemetry or model traffic whatsoever, a normal
cloud-backed Copilot session cannot be certified to satisfy it from this
evidence. Keep local deterministic TASC checks usable without launching
Copilot; resolve the stronger interpretation before any live integration/eval.

### Proposed opt-in ingestion policy

- Consent must identify host, execution machine, selected repository/Profile,
  time range, file/export roots, metadata fields, frequency, storage location,
  deletion/retention and any continuous-in-session collection. No blanket scan
  of `~/.copilot`; config/OAuth/MCP secret directories are out of scope.
- Prefer activation/operation metadata with local IDs and digests. Parse raw
  content only locally as needed; drop `skill.invoked.content`, prompts,
  responses, reasoning, tool arguments/results and environment values from the
  persisted projection by default. GitHub warns debug exports can contain all
  of these sensitive categories. [S7, S8, S13]
- Redact before persistence/output, using field allowlists first and secret/PII
  patterns second. Path/repository/skill names and timestamps can themselves
  identify private work. Hashing is not anonymization; pseudonymous joins
  should remain local and protected. Redaction is best effort, never permission
  to upload the remainder.
- Bound parser bytes/depth, reject malformed input as data, never execute log or
  skill instructions, avoid symlink traversal outside consented roots and
  handle partially written JSONL without modifying the source. Track gaps and
  duplicates. Session resume can repair transcripts; do not resume merely to
  inspect them. [S10]
- Keep raw imports ephemeral unless separately opted in; define finite
  retention/byte limits for derived records and evidence, local access controls,
  explicit purge/export and provenance of redaction. Do not commit usage
  artifacts or put them in synced folders by default.
- Withdrawal stops future reads immediately; offer deletion of TASC-owned
  derivatives without deleting host-owned sessions. Document that backups,
  host copies and already-synced copies require separate deletion. GitHub
  explicitly says local deletion alone does not remove synced storage; archive
  is not deletion. No universal host automatic-retention guarantee was found.
  [S8, S9]

Specific retention durations, encryption/key ownership, default field set and
revocation UX remain product decisions. Nothing here authorizes changing them
on the user's machine.

### Cost and mutations

Deterministic local checks need no model call and should not silently fetch
dependencies, install tools, start remote MCP, query account history, or run
embedded skill scripts. User-approved mutation plans remain separate from
host tool approval and from paid/model Evaluation approval.

An approved Evaluation should declare tasks/Trials/retries, candidate and
Control Runs, executor/graders/tools, disclosure, output retention, concurrency,
time and usage ceilings, cancellation behavior and overshoot risk. Include
model graders and suite generation in the budget; “read-only” model analysis
still costs money/credits. Observed tokens/credits are not provider currency.
Missing usage is not zero, subscription access is not proof of zero marginal
cost, and asynchronous cancellation is not a guaranteed hard spending cap.
Waza's native comparison guide explicitly warns of usage overshoot and missing
runtime/currency evidence. [S16]

## Measuring effectiveness without inventing causality

### Matched-control proposal

1. Predeclare held-out Evaluation Tasks, outcomes, deterministic graders where
   appropriate, quality rubric, negative controls, primary metric, repetitions
   and stopping/budget rules. A mock run checks wiring, not agent quality. [S15]
2. Use isolated workspaces and fresh sessions for each Trial. Match input bytes,
   permissions, tools/MCP, other skills/instructions, host/runtime, model
   selection, evaluation settings and resource limits. No production side
   effects. Change only the candidate skill's availability/injection.
3. Randomize/interleave skill-on versus skill-off order and block comparisons
   by task/environment/time. Record actual activation separately. An **available
   skill** experiment tests discovery plus downstream behavior; a **forced
   injection** experiment tests different behavior and must be labelled as such.
4. Verify Control Runs cannot see the candidate through duplicate install
   locations, plugin/agent preloads, project instructions, conversation memory,
   copied resources or cached outputs. Do not disable a real user's skill as an
   experimental side effect. If the host cannot isolate discovery, label the
   comparison contaminated/unassessed.
5. Repeat enough independent Trials to show distributions, paired differences
   and uncertainty; retain failures/timeouts/cancellations and report retry
   policy. Attempts within one Trial are not independent samples. Do not
   discard poor results or count retry-until-success as first-pass success.
6. Report task success, rubric outcomes, observed elapsed time and token/credit
   usage separately. Blinded human or calibrated model grading can supplement
   deterministic checks, but model grading needs separate disclosure/spend
   approval. Do not collapse safety, correctness, speed and cost into one
   unqualified “skill score.”

This is an experimental-design proposal, not a measured result. Primary
evaluation guidance identifies nondeterministic outputs and recommends
task-specific datasets and calibration against human judgment [S19].
Even fixed prompts/settings cannot freeze provider updates, Auto routing,
sampling, tool/network variability, rate limiting, cache warmth, repository
state, concurrent load, skill interactions or user intervention.

Observational imported sessions add selection bias, task difficulty, survivor
bias and incomplete coverage. “Successful sessions used X more often” cannot
identify X's causal effect. A well-run randomized matched experiment supports
a **bounded contribution estimate under tested conditions**, not a guarantee
for the installed skill across all projects/hosts.

### Waza feasibility and current gap

Current pinned Waza docs describe independent `trials_per_task`, within-Trial
`max_attempts`, final-attempt evidence limits and ordinary Copilot SDK execution.
They also describe v0.39.0 controlled comparisons, but distinguish mock/offline
collection from explicitly opted-in, potentially paid native Copilot collection.
[S15, S16]

Crucially, the strict `native-copilot` profile is fixed-model, single-turn,
text-check only, **no skills, hooks, MCP, repositories, concurrency, resume or
multiturn**. Its deny-all client tools are **not server isolation**. Therefore:

- Do not claim current strict Waza comparison already measures skill-on/off
  contribution or establishes installation-bound effectiveness.
- Ordinary skill Evaluations plus externally matched Control Runs may be a
  practical future route, but require version-pinned artifact/capability
  verification, isolation and an explicitly designed comparison contract.
- Never upgrade structurally matched snapshots, mock success or a supplied
  “pass” label into causal/release assurance. Missing required evidence must
  remain nonpassing or inconclusive. [S16]

No Waza command or live/model Evaluation was run for this research.

## Open product decisions and verification gates

1. **Initial supported target:** CLI first, VS Code Copilot harness too, or
   both? Specify tested versions/builds, execution-host boundaries and treatment
   of Preview hooks/debug capture. App parity needs its own admission test.
2. **Integration minimum:** Command Skill alone, local MCP, optional hooks, or
   extension watcher? None implies permission to install/configure another.
3. **“No cloud” semantics:** no TASC exporter versus no host session sync versus
   no service telemetry. Define behavior when enterprise settings prevent
   enforcing the requested local-only boundary.
4. **Consent/storage:** fields, scope/frequency, retention, raw-content handling,
   purge/export and local protection; distinguish metadata ingestion approval
   from disclosure to the user's primary assistant.
5. **Evidence confidence:** which host/version event schemas are admitted;
   identity ambiguity, coverage gaps and invalidation policy; no-event cases
   must not generate “unused skill” removal recommendations automatically.
6. **Quiet-check policy:** dependency scope, debounce/time budgets, bounded
   rescan, nonblocking failures, actionable notification thresholds and
   distinction between assessment computation and strictly read-only Audit.
7. **Evaluation approval:** acceptable fixture disclosure, model/executor,
   graders, budget/overshoot, runtime/tool isolation, matched controls and
   statistical sufficiency. Current private-content/model authorization is
   absent; do not run these experiments.
8. **Consistency evidence:** how Ownership Claims and Precedence Rules are
   authored/approved; what counts as deterministic contradiction versus
   speculative semantic conflict. No automatic skill rewrites.

Before implementation, an approved, synthetic/non-sensitive compatibility
exercise should establish: discover/invoke exact Installation; observe
`skill.invoked` including trigger/source; check persisted versus ephemeral
events across resume/compaction/subagents; verify hook input/error/timeout
behavior and edit/failure/out-of-band coverage; confirm effective local-only
settings without changing them; inspect redacted exports and prove denied
mutations/model calls remain denied. This is a future gate, **not performed
or authorized by this ticket**.

## Primary sources

All accessed 2026-10-23. GitHub and VS Code rendered docs were checked alongside
the pinned source below. Public Microsoft repository API access returned SAML
403; public git ref lookup and raw source retrieval worked, so it did not block
research. Main-branch source observations are explicitly not installed-runtime
verification.

- **S1 — CLI Agent Skills:** [rendered](https://docs.github.com/en/copilot/how-tos/copilot-cli/customize-copilot/add-skills);
  [GitHub docs snapshot](https://github.com/github/docs/blob/935221fd662811fa04955b1759bf67b96b7ae028/content/copilot/how-tos/copilot-cli/customize-copilot/add-skills.md).
- **S2 — Agent Skills surfaces:** [rendered](https://docs.github.com/en/copilot/concepts/customizations/agent-skills);
  [snapshot](https://github.com/github/docs/blob/935221fd662811fa04955b1759bf67b96b7ae028/content/copilot/concepts/customizations/agent-skills.md).
- **S3 — VS Code skills, discovery and harness differences:** [rendered](https://code.visualstudio.com/docs/agent-customization/agent-skills);
  [snapshot](https://github.com/microsoft/vscode-docs/blob/a03090533810eb02e5b9a2a650e2a3df32192b42/docs/agent-customization/agent-skills.md).
- **S4 — CLI MCP:** [rendered](https://docs.github.com/en/copilot/how-tos/copilot-cli/customize-copilot/add-mcp-servers);
  [snapshot](https://github.com/github/docs/blob/935221fd662811fa04955b1759bf67b96b7ae028/content/copilot/how-tos/copilot-cli/customize-copilot/add-mcp-servers.md).
- **S5 — Copilot hook inputs, locations, failure behavior and cloud limits:** [rendered](https://docs.github.com/en/copilot/reference/hooks-reference);
  [snapshot](https://github.com/github/docs/blob/935221fd662811fa04955b1759bf67b96b7ae028/content/copilot/reference/hooks-reference.md);
  [CLI setup/restart behavior](https://docs.github.com/en/copilot/how-tos/copilot-cli/customize-copilot/use-hooks).
- **S6 — VS Code hooks (Preview), shared Copilot versus Local:** [rendered](https://code.visualstudio.com/docs/agent-customization/hooks);
  [snapshot](https://github.com/microsoft/vscode-docs/blob/a03090533810eb02e5b9a2a650e2a3df32192b42/docs/agent-customization/hooks.md).
- **S7 — Released SDK event contract:** [streaming/persistence semantics and `skill.invoked`](https://github.com/github/copilot-sdk/blob/f527e6134f9b38980af8aa796b4c959458a88769/docs/features/streaming-events.md);
  [generated activation types, including optional trigger/source/model](https://github.com/github/copilot-sdk/blob/f527e6134f9b38980af8aa796b4c959458a88769/nodejs/src/generated/session-events.ts#L9072-L9118);
  [GA status](https://github.com/github/copilot-sdk/blob/f527e6134f9b38980af8aa796b4c959458a88769/README.md#is-the-sdk-production-ready).
- **S8 — CLI storage/settings/sync defaults:** [rendered](https://docs.github.com/en/copilot/reference/copilot-cli-reference/cli-config-dir-reference);
  [snapshot](https://github.com/github/docs/blob/935221fd662811fa04955b1759bf67b96b7ae028/content/copilot/reference/copilot-cli-reference/cli-config-dir-reference.md).
- **S9 — Session location, default sync, privacy, history-model disclosure and deletion:** [rendered](https://docs.github.com/en/copilot/concepts/security-governance-and-network-settings/session-data);
  [snapshot](https://github.com/github/docs/blob/935221fd662811fa04955b1759bf67b96b7ae028/content/copilot/concepts/security-governance-and-network-settings/session-data.md).
- **S10 — SDK capabilities and recovery boundaries:** [compatibility matrix](https://github.com/github/copilot-sdk/blob/f2b720c8ec368e035a4d5c72b012fd41b0d533e4/docs/troubleshooting/compatibility.md);
  [session persistence/transcript recovery](https://github.com/github/copilot-sdk/blob/f2b720c8ec368e035a4d5c72b012fd41b0d533e4/docs/features/session-persistence.md).
  Source snapshot dated 2026-10-22; individual experimental surfaces require release verification.
- **S11 — CLI command flags and OTel:** [rendered](https://docs.github.com/en/copilot/reference/copilot-cli-reference/cli-command-reference#opentelemetry-monitoring);
  [snapshot](https://github.com/github/docs/blob/935221fd662811fa04955b1759bf67b96b7ae028/content/copilot/reference/copilot-cli-reference/cli-command-reference.md).
- **S12 — IDE feature matrix (public preview):** [rendered](https://docs.github.com/en/copilot/reference/copilot-feature-matrix);
  [snapshot](https://github.com/github/docs/blob/935221fd662811fa04955b1759bf67b96b7ae028/content/copilot/reference/copilot-feature-matrix.md).
- **S13 — VS Code debug capture, local persistence, exports and sensitive content:** [rendered](https://code.visualstudio.com/docs/agents/agent-troubleshooting/chat-debug-view);
  [snapshot](https://github.com/microsoft/vscode-docs/blob/a03090533810eb02e5b9a2a650e2a3df32192b42/docs/agents/agent-troubleshooting/chat-debug-view.md).
- **S14 — VS Code MCP trust, forwarding and execution environment:** [rendered](https://code.visualstudio.com/docs/agents/run/manage-mcp-servers);
  [snapshot](https://github.com/microsoft/vscode-docs/blob/a03090533810eb02e5b9a2a650e2a3df32192b42/docs/agents/run/manage-mcp-servers.md).
- **S15 — Waza task/Trial/Attempt and mock semantics:** [pinned concepts](https://github.com/microsoft/waza/blob/1b763c094145f2dd7e5148898bee957737c6922b/site/src/content/docs/concepts.mdx);
  [schema](https://github.com/microsoft/waza/blob/1b763c094145f2dd7e5148898bee957737c6922b/site/src/content/docs/reference/schema.mdx).
  Public HEAD resolved 2026-10-23; docs identify v0.39.0 availability, not this TASC checkout's installed version.
- **S16 — Waza controlled comparisons, explicit native profile restrictions and spend limits:** [pinned guide](https://github.com/microsoft/waza/blob/1b763c094145f2dd7e5148898bee957737c6922b/site/src/content/docs/guides/controlled-comparisons.mdx).
- **S17 — VS Code extension filesystem watcher API:** [API reference](https://code.visualstudio.com/api/references/vscode-api#workspace.createFileSystemWatcher);
  [pinned API source](https://github.com/microsoft/vscode/blob/53b7b197990e6d9354dba0e42cce9b34d7c56c5c/src/vscode-dts/vscode.d.ts#L14079).
  Public source HEAD resolved 2026-10-23.
- **S18 — Copilot app customization:** [rendered](https://docs.github.com/en/copilot/how-tos/github-copilot-app/customize-github-copilot-app);
  [snapshot](https://github.com/github/docs/blob/935221fd662811fa04955b1759bf67b96b7ae028/content/copilot/how-tos/github-copilot-app/customize-github-copilot-app.md).
- **S19 — Primary model-provider evaluation methodology (not a proposed provider/integration):**
  [OpenAI evaluation best practices](https://developers.openai.com/api/docs/guides/evaluation-best-practices),
  accessed 2026-10-23; live documentation, not revision-pinned. Used only for
  variability, task-specific design and human calibration. No OpenAI calls or
  recommendation to adopt its Evals platform; that page itself documents platform deprecation.
