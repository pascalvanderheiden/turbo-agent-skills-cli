# TASC v3: skill-manager evidence and task-first journeys

- **Research ticket:** [#134](https://github.com/pascalvanderheiden/turbo-agent-skills-cli/issues/134).
- **Observed:** 2026-10-23. Research/charting only; no implementation, runtime experiment, or final product decision.
- **Scope:** competitor feature/UX comparison and candidate task journeys. Copilot hook/log feasibility belongs to the separate research track.
- **Local baseline:** [`f3ab05f64a99595cec491d86685ad96223a098e3`][T1], including the root domain vocabulary and agent instructions. There is no `docs/adr/` directory at this baseline.

## Summary

The strongest combination to consider is **Copilot conversation for task intent, a reliable TASC CLI for authorized operations, and an evidence-led overview**. This is a recommendation for exploration, not a decision to replace existing surfaces.

- **ASM (`luongnv89/asm`)** offers useful scriptable inventory, source-aware operations, optional terminal browsing, static checks, and separate machine envelopes. Its “semantic overlap” is deterministic text similarity; its ordinary `eval` is static assessment, not demonstrated task effectiveness. Its initializer writes an incomplete template directly into a tool directory by default. [A1], [A5], [A7], [A11], [A12]
- **Skills Manager (`xingkongliang/skills-manager`)** is a different, desktop-first product with a central library, explicit deployment, multi-agent selection, source comparison, presets, and a real shared-core CLI plus `manage-skills` Agent Skill. It is **not GUI-only**. Its update protection has an important boundary: preventing removed paths is not protection against overwriting edits to surviving files. [M1], [M4], [M6], [M7]
- Neither inspected management surface establishes that installations, popularity, approximate context cost, or a static quality score prove effectiveness. Agent Skills' first-party evaluation guidance instead recommends matched with/without-skill runs and evidence-backed grading. [A7], [A8], [M7], [S3]
- Borrow progressive disclosure, explicit target selection, status/action separation, and inspect-before-apply. Do **not** borrow one-click mutations, automatic sync, ambiguous “audit,” blanket shell approval, or popularity-as-trust defaults. [A4], [M1], [M4], [G1], [G3]

## Method and evidence rules

Only repository text, source code, specifications, and first-party documentation were read, through GitHub's API and official web documentation. No competitor package was installed, built, imported, or executed; no skill instructions or embedded shell examples were run.

Labels used below:

- **D — documented:** the owner describes the behavior; not independently exercised.
- **I — inspected:** a named code path supports the statement; not runtime verification.
- **U — unverified/not established:** absent from the inspected surface or dependent on untested runtime behavior. This is not proof that no other code or extension provides it.
- **P — proposal:** a TASC journey or acceptance criterion, not an existing capability claim.

Repository revisions are immutable. Web pages are access-dated observations, not immutable snapshots. Context7 was used for current Copilot CLI documentation discovery (`/github/copilot-cli`). Some returned snippets cited `_autodocs/...` files not present in the pinned repository tree; those snippets are **not evidence** here. Claims were followed to GitHub-owned docs/source instead. The pinned Copilot README also uses older premium-request wording, whereas the newer pinned GitHub docs describe token-based AI credits; this report does not infer billing from that README. [G1], [G4]

### Revision register

| Source | Pinned revision | Commit date (UTC) | Role |
| --- | --- | --- | --- |
| `luongnv89/asm` | [`10d9bac8ce34e4b2f4ea55c81166bb2777edb2a9`][AR] | 2026-10-20 | CLI/TUI competitor; MIT at inspection |
| `xingkongliang/skills-manager` | [`7a2219219287f4ef4c543fc169ebcc5b0bff085c`][MR] | 2026-10-21 | Desktop/shared-core CLI competitor; MIT at inspection |
| `agentskills/agentskills` | [`69ef37e9424c0a7ea9dd2293b559e43ec8176379`][SR] | 2026-08-09 | Format specification and separately identified implementation/evaluation guidance |
| `github/docs` | [`935221fd662811fa04955b1759bf67b96b7ae028`][GR] | 2026-10-23 | Copilot CLI workflow, approval and skill documentation |
| `github/copilot-cli` | [`7ea98dfc33c68aaf6443ea8fc054e20ff7fcd120`][CR] | 2026-10-22 | Public README cross-check; not a complete implementation audit |
| `cli-guidelines/cli-guidelines` | [`697d6a29fc8c93d3981a755c0c7683507ad39c3e`][LR] | 2026-05-16 | Author-owned CLI design guidance, not a product guarantee |
| `openai/codex` | [`d6817454d76885e9d0e118bbf82e507aed6f715a`][OR] | 2026-10-23 | `docs/exec.md` redirects to official live docs; not used to claim pinned implementation behavior |

## Competitor evidence matrix

The two repositories below are deliberately separate. ASM is not another name for Skills Manager. Advertised catalog/tool counts are mutable marketing observations, not an independently verified compatibility census. [A1], [M1]

| Concern | ASM | Skills Manager | Implication for TASC (P) |
| --- | --- | --- | --- |
| Entry and task orientation | **D/I:** CLI-first README; optional no-argument TUI. Dashboard shows counts, scope, tools, duplicates and navigation. Command examples organize operational tasks, but do not establish a guided intent-to-outcome journey. [A1], [A2] | **D/I:** desktop dashboard summarizes library, deployment coverage and enabled agents; offers scan/import, install and agent-control setup. Central-library and workspace organization dominates. [M1], [M2] | Start with “What are you trying to do?” and offer a small set of tasks; retain explicit CLI access and overview. Do not require a second agent conversation for every check. |
| Discovery | **I:** `search` combines installed results and an index, supports tags/status filters and a bounded local fuzzy-search path. Available results matching installed names are hidden by name. This is not verified intent understanding and can obscure distinct same-name origins. [A3] | **D/I:** marketplace keyword search; CLI calls `skills.sh/api/search`; local UI normalizes hyphens, underscores and whitespace before substring matching name/description. Returned installs are popularity data. [M3], [M7] | Separate strict **Search** from intent-oriented **Discover**; show matched capabilities, scope constraints, source identity and uncertainty. Never equate same name with same Skill Identity. |
| Authoring | **I:** `init` scaffolds in the selected tool's global directory unless `--path` is supplied; template has an empty description. Existing paths require confirmation/force. No guided goal elicitation in this command. [A5] | **I/U:** inspected CLI enum has install/import, source, deployment and organization operations, but no create/scaffold/evaluate command. Preview/diff exists; a guided original-authoring journey was not established. [M4], [M6] | Create a **Skill Draft** in an explicit **Skill Workspace**, not an Installation. Derive trigger, non-goals and examples from the task, validate, then offer separate next steps. |
| Overview | **I:** grouped/flat inventory, scope/tool summaries, TUI counters, JSON and machine output. Cross-tool grouping reduces repetition. [A2], [A9] | **D/I:** library, global/agent/project workspaces; badges and target selection expose distribution. Dashboard coverage is “library skills with targets / library skills,” not observed execution coverage. [M1], [M2], [M3] | Group by Skill Identity, expand to Installations. Keep tracked/untracked, watched, source confidence, local changes and evidence age separate; do not call deployment “usage.” |
| Installation and scope | **D/I:** tool selection, install inspection/security stages, optional library/activation and linking are exposed. [A1], [A10] | **D/I:** install is library-only by default; explicit deploy selects agents. Global/project scopes and symlink/copy are documented. Deployment ownership policies distinguish absent, foreign, recorded and explicitly confirmed targets. [M1], [M4], [M5], [M7] | Put exact Adapter Target, path, scope, revision and write set in a bounded approval plan. A shared directory can affect several assistants; do not label it Copilot-only without verifying that boundary. |
| Updates and drift | **I:** updater fetches a resolved commit when supplied, replaces through backup/copy, writes lock metadata before discarding backup, and attempts rollback on failure. This supports pinning and recovery, not a guarantee that local edits survive a successful replacement. [A6] | **D/I:** source metadata, upstream comparisons and content diff; removed paths require a revision-bound removal approval, and CLI reports held-back removals. The management skill explicitly warns that edits to paths also shipped upstream can be silently overwritten. [M6], [M7] | Review a per-file diff and local drift before approval; bind approval to old/new revisions and local fingerprints. Reject changed preconditions. Recovery is not the same as preserving user changes. |
| Duplicates/conflicts | **I:** duplicate audit has content-fingerprint guards before automatic removal; `--force` bypasses divergent-copy guard. “Semantic overlap” uses offline token-based Jaccard with stopword/IDF weighting. Separate same-repository dedupe prioritizes conventional paths by name. [A4], [A11] | **I:** ownership-aware deployment refusal; source diff and project divergence presentation. **U:** no equivalent instruction-level Consistency Check or deterministic ownership-claim model established. [M2], [M5], [M6] | Separate same identity, equal content, destination collision, trigger/capability overlap and incompatible instructions. Only the first three may admit deterministic conclusions; inferred overlap remains evidence with confidence. |
| Quiet/read-only operations | **I:** machine envelope and stderr redirection are useful. In duplicate audit, JSON/machine branches return before removal, while human `audit --yes` removes copies. Overlap/residency paths are read-only. Thus output mode can change semantics. [A4], [A12] | **I:** check/update are separate and many commands support dry-run. However source checks persist status columns; they are non-installing, not zero-write reads. Optional background source checks/apply also exist. [M4], [M8], [M9] | Output selection must not authorize or suppress mutations. Distinguish cached Audit from explicit evidence refresh. Quiet means no chatter, not hidden failure, network traffic, state changes or paid model invocation. |
| Evidence and assessment | **I:** ordinary `eval` invokes registered providers; inspected quality provider wraps a static `SKILL.md` linter. Best-practice provider adds opinionated checks. `--fix` mutates, with `--dry-run` preview. [A7] | **D/I/U:** local operation history, upstream metadata/diffs and deployment state are documented/implemented; no scenario-based controlled effectiveness assessment established in inspected CLI. [M1], [M4], [M6] | Distinguish deterministic format validation, Security/Consistency Findings, observational usage and **Evaluation** results. Do not collapse them into one green quality score. |
| Context/usage claims | **D/I:** approximate resident/body tokens; residency report explicitly marks “unused” and trigger-collision signals unavailable. README's universal “every message” claim is stronger than an adapter-specific measurement. [A1], [A8] | **D/I:** CLI search emits installs; management skill calls 10K+ “battle-tested.” This is promotional advice, not evidence of safety or task success. [M7] | Use labeled estimates, not claimed billed savings. “Unknown usage” is not “unused.” Popularity may help browsing but must not establish trust. |
| Agent integration | **I:** stable versioned `--machine` envelope on inspected operations. README says every command supports JSON/yes, but inspected `init` writes a template and prints a human completion message without a JSON branch: universal parity is not established. [A5], [A12] | **D/I:** ordinary `manage-skills` Agent Skill delegates to shared-core CLI, resolves the app-published binary and stops on a broken bridge; CLI has JSON errors, dry-runs, stable references and explicit deployment. [M4], [M7] | Keep the TASC Command Skill thin; negotiate capabilities/version and fail clearly on incompatibility. Verify the operation matrix, not just a top-level “agent-ready” claim. |

### Explicit boundaries worth preserving

**ASM strengths:** structured automation, integrated local/available results, summary/grouped views, immutable update targeting, heuristic overlap without a remote model, and unavailable-data reporting. **Boundaries:** static assessment is not behavioral Evaluation; a description-token estimate is not measured billing; an unverified usage signal cannot justify disabling a skill. The default initializer's empty description does not meet the standard's required non-empty description, so “scaffolded” is not “ready.” [A3], [A5], [A6], [A7], [A8], [S1]

**Skills Manager strengths:** discoverable target selection, library versus deployment separation, source diff, ownership refusals, batch previews, and an agent-facing CLI/skill that preserves the manager's state instead of encouraging direct folder edits. **Boundaries:** automatic backup/sync is a policy choice, not an inherent requirement of skill management; the source auto-updater supports separately opted-in automatic apply. Its CLI `adopt` imports into the central library via `install_from_local`, which is not TASC's definition of **Adoption** (record an existing Installation without changing its files). Do not copy the command label without the semantics. [M1], [M4], [M7], [M8], [T1]

For both products, source inspection supports particular safeguards, **not a security certification**. ASM's inspected scanner uses regex risk patterns; Skills Manager's ownership policy protects destination objects, not the semantic safety of their instructions. No adversarial corpus, supply-chain verification experiment, full code audit or end-user study was performed. [A13], [M5]

## First-party standards and agentic CLI UX

### Agent Skills: portable content, not a manager protocol

**D:** the format requires a directory containing `SKILL.md`, YAML `name` and non-empty `description`, and Markdown instructions; scripts/references/assets are optional. `allowed-tools` is experimental and host-dependent. The format does not define TASC Profiles, installation records, manager approval tokens, paid evaluation policy, or effectiveness certification. [S1]

**D:** the separate client guide describes progressive disclosure: catalog metadata first, full instructions on activation, resources as needed. It recommends bounded scans, collision warnings and trust handling. It explicitly says installation locations are not mandated by the format. Its suggested precedence and lenient loading are **implementation guidance**, not proof of Copilot's actual collision behavior and not a reason to weaken TASC's authoring validation. [S2]

**D:** evaluation guidance recommends representative prompts, expected outputs, clean contexts, with/without-skill comparisons, raw evidence and human review. Small samples should not be presented as strong statistics. This is a better basis for an optional effectiveness journey than either installation counts or static scores. [S3]

### Copilot-first does not mean bypassing the CLI or consent

**D:** GitHub documents interactive task conversation, steering, plan-before-code and a programmatic prompt interface. The same documentation warns that blanket tool approval increases risk and that directory permission scoping is heuristic. TASC's domain authorization should therefore remain meaningful even if the host permits shell execution. [G1]

The Copilot conversation itself may consume host credits. “No paid analysis” below means TASC schedules no additional model analysis or Evaluation; it does not promise that chatting with Copilot is free. A deterministic CLI read should remain usable without starting that conversation. [G1]

**D:** Copilot documents project locations `.github/skills`, `.claude/skills`, `.agents/skills` and personal locations `~/.copilot/skills`, `~/.agents/skills`; `/skills reload` and `/skills info` support verification. Host availability and a TASC Installation record are related but different observations. Plugin-managed skills must be managed through the plugin rather than removed as standalone skills. [G2], [G3]

**D:** GitHub specifically warns against pre-approving `shell`/`bash` for unreviewed skills. A first-party TASC Command Skill should not ask users to grant unrestricted shell access simply to suppress read-only prompts. How to obtain a sufficiently narrow host permission is a feasibility question for the other track. [G3]

**D:** OpenAI's current official CLI docs independently demonstrate an interactive/automation split, explicit permissions and a read-only planning option. They distinguish sandbox restrictions from approval policy; neither is synonymous with a product-level mutation plan. This is corroborating UX evidence, not a proposal to adopt Codex integration first. [O1], [O2]

**D:** the author-owned CLI Guidelines recommend stable machine output, stdout/stderr separation, explicit quiet mode, non-TTY-aware output, dry-run and stronger confirmation for dangerous actions. This is design guidance, not empirical proof that any particular arrangement is optimal. [L1]

## Proposed task journeys

Everything in this section is **P**. Operation descriptions are semantic contracts, **not newly specified executable command syntax**. Existing stable noun-first CLI/JSON contracts remain authoritative; any new grammar requires separate design and compatibility review. Domain terms follow the baseline `CONTEXT.md`. [T1]

### 1. “Help me do this task; don't install anything yet”

Example: “Review Terraform changes for Azure policy problems in this project.”

1. Copilot identifies desired output, constraints and available tools; ask only for missing information that affects the recommendation.
2. Use selected-Profile inventory and eligible cached metadata first. Separate installed matches, untracked matches and remote candidates. Do not silently scan unrelated projects/home directories.
3. Explain the top few matches: task capability, supporting description/reference, known incompatibilities, license/source confidence, observed revision, and why not alternatives. Show a local “create a skill” route if nothing fits.
4. Offer **inspect**, **use an already available skill**, **watch**, **install**, or **create** as distinct next actions. Watch changes Local State and requires its own explicit intent; inspection must not subscribe the user.
5. If remote discovery needs network or sends task text externally, disclose that boundary; prefer a derived non-sensitive query over uploading project content. A failed source remains “unavailable,” not “no relevant skills.”
6. Stop after recommendations unless the user authorizes the next operation. Inspecting metadata must not execute a candidate's scripts or obey its instructions.

**Why:** combine ASM's local/available visibility with Skills Manager's separation of library and deployment, but avoid lexical-only recommendations masquerading as task fit. [A3], [M3], [M7]

**Candidate acceptance:** a fixture with two identical names from different repositories remains distinguishable; the local-only TASC lookup makes no additional remote/model call; the explanation names evidence rather than invented effectiveness.

### 2. “Turn our repeated task into a reusable skill”

Example: “Package our release-readiness checklist; no publishing or install yet.”

1. Capture intended users, trigger/non-trigger examples, inputs, outputs, required tools, permissions and non-goals.
2. Check existing selected sources for close matches; offer reuse/adaptation versus new authoring without making the final choice.
3. Show the chosen Skill Workspace and proposed files. Obtain permission before creating/editing them; do not create inside an Installation.
4. Produce a resumable Skill Draft. Deterministically validate the Agent Skills format; present optional authoring advice separately from standard violations.
5. Show a diff and concise readiness summary. If validation fails, preserve the draft and findings, not an apparently installed partial skill.
6. End with “draft ready, not installed/evaluated/published.” Offer those as independent follow-ups. Propose Evaluation Tasks, but do not run paid Evaluations or model judges automatically.

**Why:** ASM's fast scaffolding is useful, but default installation-directory authoring and an empty description blur “created” with “usable.” Agent Skills guidance supports task/trigger descriptions and separate evidence-backed evaluation. [A5], [S1], [S3]

**Candidate acceptance:** finishing a draft does not change Adapter Targets, Watchlist, tracking or public remotes; a format pass is never worded “effective.”

### 3. “Make this skill available to Copilot here”

1. Resolve Skill Identity and Source Revision, then show the intended project/user scope, exact destination, shared-path implications and any plugin ownership.
2. Detect existing content, local modifications, ambiguous identities and destination collisions. Offer **Tracking** for an existing Installation instead of reinstalling it.
3. Present a bounded plan: source revision/digest, target paths, copy/link behavior, writes/deletions, required Security Policy decision and rollback limits.
4. Ask for explicit approval tied to that plan. Refresh/changed fingerprints invalidate it. Host shell permission alone is not TASC approval; a required Security Override is separate and reasoned.
5. Apply only approved operations through the CLI; return per-target outcomes and verify on-disk state. Report partial failure without claiming all targets succeeded.
6. Where supported, guide the user to `/skills reload` and `/skills info`; report “files installed; host reload not verified” if host verification is unavailable. Do not claim an already-running session has loaded the skill solely from a file write.

**Why:** Skills Manager's explicit targets and ownership refusals, ASM's commit pinning, and Copilot's reload/info commands provide complementary pieces. [M4], [M5], [A6], [G2]

**Candidate acceptance:** unrelated directories remain untouched; concurrent edits reject stale plans; the same normalized result appears through CLI and TASC Command Skill.

### 4. “What have I got, and what needs attention?”

Default to a short **Inventory Projection**, not a transcript of every scan and not a forced full-screen UI:

```text
Profile: project example   Evidence: cached; no refresh performed
12 Skill Identities · 17 Installations · 3 untracked · 2 watched
Attention: 1 locally modified · 2 stale checks · 1 source unavailable

Skill / stable action ID   Copilot target / scope   Tracking   Source       Evidence
release-check / skill:…    .github/skills / project tracked    verified SHA stale
policy-review / skill:…   ~/.copilot / user         untracked  unavailable  not checked
```

Numbers and identifiers above are illustrative, not observations. User-scope Installations are shown only when eligible under the explicitly selected Profile/scan scope; this mock-up does **not** propose merging project and user Profiles.

Expand a row for full path, identity, lifecycle, provenance confidence, drift, duplicate relationships and artifact details. Keep an uninstalled watched interest visibly distinct from an Installation. “Installed” means present, not invoked; “tracked” means recorded, not trusted. Use stable action identifiers, never a mutable row number in later mutations.

Offer **inspect**, **track**, **review update**, **open evidence** and **review conflict**. Keep “check now” separate from cached Audit, which must not refresh evidence. An unavailable remote or stale artifact should not become a red “broken skill” without supporting findings.

**Why:** borrow ASM grouping/counts and Skills Manager's distribution cues, but do not reinterpret deployment coverage as an effectiveness measure. [A2], [A9], [M2], [T1]

### 5. “Check these skills quietly; do not change them”

1. Name the contract: **cached Audit** reads selected Local State, installations and existing evidence; **fresh Security/Consistency Check** generates new evidence and may write an artifact. A **Watch Check** explicitly compares upstream state. “No skill-file changes” and “no writes/network/model calls anywhere” are different promises.
2. For quiet cached reads, produce a compact summary or stable JSON, no banners/spinners/pager, no repair, adoption, refresh or paid analysis. Suppress routine chatter, not actionable failures.
3. Before a fresh check, disclose scope, network/model use, artifact writes and estimated or unknown cost. Obtain separate approval for paid work. Do not run untrusted scripts merely to inspect a skill.
4. Findings include content location, rule/procedure, severity/confidence, evidence revision/time, limitations and next action. Distinguish **not checked**, **inconclusive**, **stale**, **warning**, and **blocked by policy**.
5. A requested fix produces a Remediation Proposal and a new approval boundary; checking never repairs.

**Why:** ASM's output-dependent duplicate removal and Skills Manager's metadata-writing “check” show why “read-only” must be precise. Neither operation should inherit TASC Audit semantics just because of its name. [A4], [M9], [T1]

**Candidate acceptance:** switching human/JSON/quiet output never changes side effects; cached reads leave evidence timestamps unchanged; missing evidence is not a pass.

### 6. “Update safely / explain these duplicate skills”

1. Distinguish a new Source Revision from local drift, missing files, duplicate content and task overlap. Preserve separate Installations even when they share identity.
2. Show file-level old/new/local differences. Similarity findings should quote overlapping instructions/capabilities and explain uncertainty, not propose automatic deletion.
3. Plan only selected targets, pin revisions and local fingerprints, and explain whether user changes are preserved, require a workspace edit, or block application.
4. After explicit approval, apply through the CLI and verify; expose recovery limits. Re-run only authorized checks, leaving previous artifacts linked to their original revisions.

**Why:** a fingerprint guard, backup swap or held-back deletion is useful but incomplete protection. Skills Manager explicitly documents surviving-path overwrite risk; ASM's successful replacement discards its backup after lock persistence. [A4], [A6], [M6], [M7]

**Candidate acceptance:** a user-edited surviving file is not silently replaced; equal names alone never authorize removal; upstream movement requires a new plan.

### 7. “Is this helping?” — opt-in evidence, not a leaderboard

Present a layered evidence card:

| Layer | Meaning | Must not imply |
| --- | --- | --- |
| Availability | Detected Installation and scope at observation time | Loaded into every current session |
| Observed usage, if supported and opted in | A stated event type, time window, source and coverage | “Unused” when collection was off/incomplete; successful task completion |
| Content estimates | Labeled approximate metadata/body token size | Measured billed tokens or money saved |
| Evaluation | Named scenarios, exact revisions, model/settings, trials, graders, raw results, comparable Control/Baseline Run | Universal effectiveness, security certification or causal gains from uncontrolled history |

Offer local-only, per-Profile opt-in before collecting usage evidence; explain exactly what would be retained, excluded/redacted, how long, and how to inspect/delete it. Keep this preference separate from installing the TASC Command Skill. No transcript upload or background paid Evaluation by implication. If observations cannot be attributed reliably, display “unavailable” and link the reason.

For effectiveness, propose a small representative Evaluation Suite and a cost/resource boundary first. Ask before running with-skill/Control Run trials or graders; report raw counts and costs/uncertainty alongside outcomes. “No comparable evidence yet” is an acceptable result.

**Why:** ASM explicitly lacks an “unused” signal, Skills Manager's installs are popularity, and the standard's evaluation guidance calls for controlled comparison. This defines presentation/consent needs only; it does not claim Copilot exposes the necessary events. [A8], [M7], [S3]

## Tradeoffs and unresolved decisions

These remain choices for later charting/prototyping, not decisions made by this research.

| Choice | Benefit | Cost/risk | Decision evidence still needed |
| --- | --- | --- | --- |
| Copilot-first task entry; CLI remains authoritative | Meet users in their existing conversation; reliable automation and fallback | Host/version/permission dependency; natural-language ambiguity | Other track's integration feasibility; task-completion prototype against direct CLI |
| Brief summary versus full inventory/TUI | Lower reading cost and less noise | Hiding important targets or stale evidence | Real inventory sizes; keyboard/screen-reader/narrow-terminal trial |
| Local/cached-first Discover versus always-live catalog | Privacy, offline use, predictable latency/cost | Stale or incomplete recommendations | Acceptable freshness; eligible source policy; when remote query consent is needed |
| Explainable metadata rules versus model-assisted intent ranking | Reproducible reasons and cheap checks | Weaker paraphrase handling versus model cost/data exposure | Labeled task/query set, mismatch rate and user preference; model work stays opt-in |
| Direct target install versus optional central source/library | Fewer concepts versus reuse across adapters | Copy drift versus symlink blast radius and ownership changes | Whether an optional Profile Skill Source covers needs without mandatory central ownership |
| Per-operation approval versus a reviewed batch | Fine control versus fewer interruptions | Prompt fatigue versus excessive write scope | Approval granularity, partial-failure behavior, reversibility and stale-plan UX |
| Strict standards validation versus permissive host loading | Portable ready drafts versus compatibility | Host can load content TASC rejects as incomplete | Separate error/advice taxonomy; adapter capability/version matrix |
| Usage evidence opt-in and retention | Useful local observations with user control | Incomplete attribution and sensitive data | Other research track's feasibility; retention/redaction defaults and deletion contract |
| New manager functions versus native Copilot/`gh skill` operations | TASC adds policy, provenance and comparable evidence | Duplicate lifecycle state and conflicting ownership | GitHub docs now advertise both native skill management paths; exact overlap/ownership needs separate inspection. [G2], [G3] |

### Candidate evaluation of the journeys, before implementation

Use paper/terminal prototypes with representative tasks: find a skill without installing; author a draft; adopt an existing Copilot Installation; inspect a mixed inventory; check offline; update a locally modified skill; decline paid Evaluation. Record task success, time to first useful evidence, accidental-mutation attempts, number of approvals and whether the user can explain scope/evidence uncertainty. These are proposed study measures, not measured results.

## Limitations

- Repository APIs and cited sources were accessible. OpenAI's old feature/security URLs redirected to current first-party pages; `docs/exec.md` is only a documentation pointer. Their live content is dated here, not claimed commit-pinned.
- Context7's generated-document paths could not be corroborated in the pinned Copilot tree; no hook, event schema, billing or exact CLI-version claim relies on them.
- Source inspection is selective, not exhaustive. No competitor UI, binary, network search result, installation, migration, test suite or automatic update was exercised. UI descriptions come from owner documentation and inspected components, not screenshot-based visual validation.
- README promises are not runtime guarantees. No catalog count, cross-platform adapter census, popularity quality threshold, security effectiveness or performance claim was independently validated.
- No controlled effectiveness experiment, quantitative UX study or real usage-log analysis was conducted. The report cannot rank competitors by actual outcomes or establish that any proposed journey improves completion.
- Current host skill loading, precedence, plugin ownership, permissions and billing can vary by version/policy. Runtime compatibility and Copilot event/log feasibility remain separate work; task recommendations must not imply they are solved.
- This artifact changes no product policy, command contract, adapter behavior or runtime. It does not decide the unresolved choices above.

## Sources

All repository links below use the revisions in the register. Source labels group closely related primary files; where a claim spans files, both links are provided.

[T1]: https://github.com/pascalvanderheiden/turbo-agent-skills-cli/blob/f3ab05f64a99595cec491d86685ad96223a098e3/CONTEXT.md
[AR]: https://github.com/luongnv89/asm/tree/10d9bac8ce34e4b2f4ea55c81166bb2777edb2a9
[MR]: https://github.com/xingkongliang/skills-manager/tree/7a2219219287f4ef4c543fc169ebcc5b0bff085c
[SR]: https://github.com/agentskills/agentskills/tree/69ef37e9424c0a7ea9dd2293b559e43ec8176379
[GR]: https://github.com/github/docs/tree/935221fd662811fa04955b1759bf67b96b7ae028
[CR]: https://github.com/github/copilot-cli/tree/7ea98dfc33c68aaf6443ea8fc054e20ff7fcd120
[LR]: https://github.com/cli-guidelines/cli-guidelines/tree/697d6a29fc8c93d3981a755c0c7683507ad39c3e
[OR]: https://github.com/openai/codex/tree/d6817454d76885e9d0e118bbf82e507aed6f715a

- **A1:** [ASM README][a1].
- **A2:** [ASM dashboard][a2].
- **A3:** [ASM search command][a3].
- **A4:** [ASM audit routing, output branches and removal guards][a4].
- **A5:** [ASM init command][a5]; [generated template and writer][a5b].
- **A6:** [ASM updater fetch, swap, persistence and rollback][a6].
- **A7:** [ASM eval command][a7]; [quality provider][a7b]; [best-practice provider][a7c].
- **A8:** [ASM statistics command][a8]; [residency signals and unavailable usage][a8b].
- **A9:** [ASM list grouping and output][a9].
- **A10:** [ASM install inspection][a10]; [install orchestration][a10b].
- **A11:** [ASM installed overlap algorithm][a11]; [within-repository name dedupe][a11b].
- **A12:** [ASM versioned machine envelope][a12].
- **A13:** [ASM regex code-scan patterns][a13].
- **M1:** [Skills Manager README][m1].
- **M2:** [Skills Manager dashboard][m2].
- **M3:** [Add-from-library sheet][m3]; [local text matching][m3b]; [marketplace API client][m3c].
- **M4:** [Skills Manager shared-core CLI, command enums, deployment, adoption, reports][m4].
- **M5:** [Sync engine ownership and replacement policy][m5].
- **M6:** [Skill commands: source diff, pending removals and revision-bound approval][m6]; [diff viewer][m6b].
- **M7:** [First-party `manage-skills` instructions, including update limitations][m7].
- **M8:** [Optional source auto-updater and opt-in apply][m8].
- **M9:** [Check routing and reports][m4]; [check status persistence][m6].
- **S1:** [Agent Skills format specification][s1].
- **S2:** [Agent Skills client implementation guidance][s2].
- **S3:** [Agent Skills behavioral evaluation guidance][s3].
- **G1:** [GitHub docs: Copilot CLI modes, approvals, security and billing][g1].
- **G2:** [GitHub docs: CLI skills commands, reload and plugin distinction][g2].
- **G3:** [GitHub docs reusable: skill locations, shell warning, native `gh skill` pointer][g3].
- **G4:** [Pinned Copilot CLI README][g4].
- **L1:** [CLI Guidelines source: output, interactivity, dry-run and dangerous actions][l1].
- **O1:** [OpenAI official CLI overview][o1], accessed 2026-10-23; redirected from `developers.openai.com/codex/cli/features/`.
- **O2:** [OpenAI official security/approval guidance][o2], accessed 2026-10-23; live, not pinned source code.

[a1]: https://github.com/luongnv89/asm/blob/10d9bac8ce34e4b2f4ea55c81166bb2777edb2a9/README.md
[a2]: https://github.com/luongnv89/asm/blob/10d9bac8ce34e4b2f4ea55c81166bb2777edb2a9/src/views/dashboard.tsx
[a3]: https://github.com/luongnv89/asm/blob/10d9bac8ce34e4b2f4ea55c81166bb2777edb2a9/src/commands/search.ts
[a4]: https://github.com/luongnv89/asm/blob/10d9bac8ce34e4b2f4ea55c81166bb2777edb2a9/src/commands/audit.ts
[a5]: https://github.com/luongnv89/asm/blob/10d9bac8ce34e4b2f4ea55c81166bb2777edb2a9/src/commands/init.ts
[a5b]: https://github.com/luongnv89/asm/blob/10d9bac8ce34e4b2f4ea55c81166bb2777edb2a9/src/initializer.ts
[a6]: https://github.com/luongnv89/asm/blob/10d9bac8ce34e4b2f4ea55c81166bb2777edb2a9/src/updater-updates.ts
[a7]: https://github.com/luongnv89/asm/blob/10d9bac8ce34e4b2f4ea55c81166bb2777edb2a9/src/commands/eval.ts
[a7b]: https://github.com/luongnv89/asm/blob/10d9bac8ce34e4b2f4ea55c81166bb2777edb2a9/src/eval/providers/quality/v1/index.ts
[a7c]: https://github.com/luongnv89/asm/blob/10d9bac8ce34e4b2f4ea55c81166bb2777edb2a9/src/eval/providers/skill-best-practice/v1/index.ts
[a8]: https://github.com/luongnv89/asm/blob/10d9bac8ce34e4b2f4ea55c81166bb2777edb2a9/src/commands/stats.ts
[a8b]: https://github.com/luongnv89/asm/blob/10d9bac8ce34e4b2f4ea55c81166bb2777edb2a9/src/residency.ts
[a9]: https://github.com/luongnv89/asm/blob/10d9bac8ce34e4b2f4ea55c81166bb2777edb2a9/src/commands/list.ts
[a10]: https://github.com/luongnv89/asm/blob/10d9bac8ce34e4b2f4ea55c81166bb2777edb2a9/src/commands/install-inspect.ts
[a10b]: https://github.com/luongnv89/asm/blob/10d9bac8ce34e4b2f4ea55c81166bb2777edb2a9/src/commands/install-run.ts
[a11]: https://github.com/luongnv89/asm/blob/10d9bac8ce34e4b2f4ea55c81166bb2777edb2a9/src/installed-overlap.ts
[a11b]: https://github.com/luongnv89/asm/blob/10d9bac8ce34e4b2f4ea55c81166bb2777edb2a9/src/skill-dedupe.ts
[a12]: https://github.com/luongnv89/asm/blob/10d9bac8ce34e4b2f4ea55c81166bb2777edb2a9/src/utils/machine.ts
[a13]: https://github.com/luongnv89/asm/blob/10d9bac8ce34e4b2f4ea55c81166bb2777edb2a9/src/security-auditor-scan.ts
[m1]: https://github.com/xingkongliang/skills-manager/blob/7a2219219287f4ef4c543fc169ebcc5b0bff085c/README.md
[m2]: https://github.com/xingkongliang/skills-manager/blob/7a2219219287f4ef4c543fc169ebcc5b0bff085c/src/views/Dashboard.tsx
[m3]: https://github.com/xingkongliang/skills-manager/blob/7a2219219287f4ef4c543fc169ebcc5b0bff085c/src/components/AddSkillsSheet.tsx
[m3b]: https://github.com/xingkongliang/skills-manager/blob/7a2219219287f4ef4c543fc169ebcc5b0bff085c/src/lib/skillSearch.ts
[m3c]: https://github.com/xingkongliang/skills-manager/blob/7a2219219287f4ef4c543fc169ebcc5b0bff085c/src-tauri/src/core/skillssh_api.rs
[m4]: https://github.com/xingkongliang/skills-manager/blob/7a2219219287f4ef4c543fc169ebcc5b0bff085c/src-tauri/src/bin/skills-manager-cli.rs
[m5]: https://github.com/xingkongliang/skills-manager/blob/7a2219219287f4ef4c543fc169ebcc5b0bff085c/src-tauri/src/core/sync_engine.rs
[m6]: https://github.com/xingkongliang/skills-manager/blob/7a2219219287f4ef4c543fc169ebcc5b0bff085c/src-tauri/src/commands/skills.rs
[m6b]: https://github.com/xingkongliang/skills-manager/blob/7a2219219287f4ef4c543fc169ebcc5b0bff085c/src/components/SkillSourceDiffViewer.tsx
[m7]: https://github.com/xingkongliang/skills-manager/blob/7a2219219287f4ef4c543fc169ebcc5b0bff085c/skills/manage-skills/SKILL.md
[m8]: https://github.com/xingkongliang/skills-manager/blob/7a2219219287f4ef4c543fc169ebcc5b0bff085c/src-tauri/src/core/skill_auto_updater.rs
[m9]: https://github.com/xingkongliang/skills-manager/blob/7a2219219287f4ef4c543fc169ebcc5b0bff085c/src-tauri/src/commands/skills.rs
[s1]: https://github.com/agentskills/agentskills/blob/69ef37e9424c0a7ea9dd2293b559e43ec8176379/docs/specification.mdx
[s2]: https://github.com/agentskills/agentskills/blob/69ef37e9424c0a7ea9dd2293b559e43ec8176379/docs/client-implementation/adding-skills-support.mdx
[s3]: https://github.com/agentskills/agentskills/blob/69ef37e9424c0a7ea9dd2293b559e43ec8176379/docs/skill-creation/evaluating-skills.mdx
[g1]: https://github.com/github/docs/blob/935221fd662811fa04955b1759bf67b96b7ae028/content/copilot/concepts/copilot-surfaces/copilot-cli.md
[g2]: https://github.com/github/docs/blob/935221fd662811fa04955b1759bf67b96b7ae028/content/copilot/how-tos/copilot-cli/customize-copilot/add-skills.md
[g3]: https://github.com/github/docs/blob/935221fd662811fa04955b1759bf67b96b7ae028/data/reusables/copilot/creating-adding-skills.md
[g4]: https://github.com/github/copilot-cli/blob/7ea98dfc33c68aaf6443ea8fc054e20ff7fcd120/README.md
[l1]: https://github.com/cli-guidelines/cli-guidelines/blob/697d6a29fc8c93d3981a755c0c7683507ad39c3e/content/_index.md
[o1]: https://learn.chatgpt.com/docs/codex/cli
[o2]: https://learn.chatgpt.com/docs/agent-approvals-security
