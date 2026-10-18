# Research: Agent Skill lifecycle landscape — `gh skill`, agentskills.in, asm, and GitHub conventions

- **Wayfinder issue:** [#6 — Research existing skill managers and gh skill overlap](https://github.com/pascalvanderheiden/turbo-agent-skills-cli/issues/6) (part of map [#1](https://github.com/pascalvanderheiden/turbo-agent-skills-cli/issues/1))
- **Question:** Which lifecycle, discovery, sharing, update, and watchlist capabilities are already covered by `gh skill`, agentskills.in, asm, and common GitHub repository conventions, and where is TASC's differentiated product boundary?
- **Research date:** 2026-10-16
- **Method:** Primary sources only — `gh` CLI preview help output captured directly from the installed binary, the `cli/cli` GitHub repository source and release notes, the `agentskills.io` specification site, and the GitHub Blog changelog announcement. `agentskills.in` (branded "agentskills.in", serving from `agentskills.in`/`www.agentskills.in`) and `github.com/luongnv89/asm` were inspected as **examples of existing product shapes only**, per the map's explicit framing — not as authorities on what TASC should do.

---

## 1. `gh skill` — current capabilities (primary source)

### 1.1 Status and provenance

`gh skill` (alias `gh skills`) is a **public-preview**, built-in command group shipped directly in the `cli/cli` GitHub CLI binary — not a third-party extension. Verified directly from the installed CLI:

```
$ gh --version
gh version 2.101.0 (2026-09-15)
$ gh skill --help
Install and manage agent skills from GitHub repositories.
Working with agent skills in the GitHub CLI is in preview and
subject to change without notice.
```

It was introduced in `gh` **v2.90.0** (2026-04-16) per the GitHub CLI release notes:

> "## Manage agent skills with `gh skill` (Public Preview) — [Agent skills](https://agentskills.io) are portable sets of instructions, scripts, and resources that teach AI coding agents how to perform specific tasks. The new `gh skill` command makes it easy to discover, install, manage, and publish agent skills from GitHub repositories - right from the CLI."
— [cli/cli release v2.90.0](https://github.com/cli/cli/releases/tag/v2.90.0), announced on the [GitHub Blog, 2026-04-16](https://github.blog/changelog/2026-04-16-manage-agent-skills-with-github-cli/)

Implementation entry point confirmed in source: `cli/cli:pkg/cmd/skills/skills.go` registers exactly six subcommands — `install`, `list`, `preview`, `publish`, `search`, `update` — via `NewCmdSkills`, added in [cli/cli#13165](https://github.com/cli/cli/pull/13165). There is **no seventh subcommand** (e.g., no `watch`, `evaluate`, or `check`) anywhere in the package tree (`gh api /repos/cli/cli/contents/pkg/cmd/skills` lists only `install/`, `list/`, `preview/`, `publish/`, `search/`, `update/`, plus `skills.go`/`skills_test.go`).

### 1.2 Subcommand-by-subcommand capability matrix (from live `--help` output)

| Subcommand | What it does | Key flags / behavior | Source |
|---|---|---|---|
| `gh skill search <query>` | GitHub Code Search–backed search for `SKILL.md` files across all public repos, ranked by name-match relevance | `--owner`, `--limit`/`-L`, `--page`, `--json` (fields: `description, namespace, path, repo, skillName, stars`), `--jq`, `--template` | `gh skill search --help` |
| `gh skill preview <repo> [<skill>]` | Renders `SKILL.md` in a pager without installing; shows file tree first, then content; interactive file picker for scripts/references | version pin via `@VERSION` (tag/branch/SHA); `--allow-hidden-dirs` | `gh skill preview --help` |
| `gh skill install <repo> [<skill[@version]>]` | Installs a skill into a host-specific directory (project or user scope) for a target agent | `--agent` (37 agent hosts listed, incl. `github-copilot`, `claude-code`, `cursor`, `codex`, `gemini-cli`, etc.), `--scope {project\|user}` (default `project`), `--dir`, `--pin`, `--force`/`-f`, `--all`, `--from-local`, `--allow-hidden-dirs`, `--upstream` | `gh skill install --help` |
| `gh skill list` | Lists installed skills across all known agent host directories, project + user scope | `--agent`, `--scope`, `--dir`, `--json` (fields: `agentHosts, path, pinned, scope, skillName, sourceURL, version`), `--jq`, `--template` | `gh skill list --help`; added in v2.94.0 ([cli/cli#13418](https://github.com/cli/cli/pull/13418)) |
| `gh skill update [<skill>...]` | Compares local tree SHA (from `SKILL.md` frontmatter) against the remote repo and re-downloads changed skills | `--all`, `--dry-run`, `--force`, `--unpin`, `--dir`; pinned skills skipped by default; skills without GitHub metadata are skipped non-interactively | `gh skill update --help` |
| `gh skill publish [<dir>]` | Validates a local repo's skills against the Agent Skills spec and creates a GitHub Release to publish them | `--dry-run`, `--fix` (strips install metadata), `--tag`; checks name/directory match, required frontmatter, `allowed-tools` type, and strips `metadata.github-*` install metadata; interactively adds the `agent-skills` topic and generates release notes | `gh skill publish --help` |

### 1.3 Discovery and installation conventions it already encodes

- **Discovery glob:** `skills/*/SKILL.md`, including when nested under a prefix (e.g. `terraform/code-generation/skills/...`), plus (per `publish --help`) `skills/{scope}/*/SKILL.md`, root-level `*/SKILL.md`, and `plugins/{scope}/skills/*/SKILL.md`. Hidden directories (`.claude/skills/`, `.agents/skills/`, `.github/skills/`) require `--allow-hidden-dirs`, added in [cli/cli#13213](https://github.com/cli/cli/pull/13213) (v2.91.0) and extended to `preview` in v2.92.0 ([cli/cli#13265](https://github.com/cli/cli/pull/13265)).
- **Version resolution order** when no version is given: (1) latest tagged release, (2) default branch HEAD; pin with `@VERSION` or `--pin` (git tag or commit SHA).
- **Shared install path for Copilot-family agents:** "several agents (including GitHub Copilot, Cursor, Codex, Gemini CLI, Antigravity, Amp, Cline, OpenCode, and Warp) share the `.agents/skills` directory," deduplicated if multiple `--agent` values resolve to the same destination (`gh skill install --help`).
- **Source-tracking metadata:** install injects frontmatter metadata identifying the source repository, which `gh skill update` later diffs by tree SHA; `publish` strips this (`metadata.github-*`) before release so it never ends up in the canonical published copy.
- **Re-publish detection:** install can detect when a target skill was re-published from an upstream source and offers (`--upstream`) to install from the original, added in [cli/cli#13236](https://github.com/cli/cli/pull/13236) (v2.91.0).
- **GHEC data-residency support** added in v2.92.0 ([cli/cli#13264](https://github.com/cli/cli/pull/13264)) — all five network-touching subcommands work against GitHub Enterprise Cloud hosts.
- **Security note:** v2.97.0 and v2.102.0 release notes disclose and fix terminal-escape-sequence injection vulnerabilities in `gh skill preview`/`search` output and in interactive search→install argument passing — evidence that upstream `gh` actively hardens this surface and TASC should not re-implement raw rendering/parsing of untrusted `SKILL.md`/search content without equivalent care. ([cli/cli v2.97.0](https://github.com/cli/cli/releases/tag/v2.97.0), [cli/cli v2.102.0](https://github.com/cli/cli/releases/tag/v2.102.0))

### 1.4 What `gh skill` explicitly does **not** do

Confirmed by absence in both the help text and the source package listing:
- **No watchlist/notification primitive.** Nothing tracks "skills I care about but haven't installed," notifies on new upstream releases, or diffs against a followed-but-not-installed set. `update` only operates on skills already installed.
- **No evaluation or quality-scoring.** No command measures whether a skill "works," benchmarks it, or produces a pass/fail report (TASC's Waza-based Evaluation capability from map [#1](https://github.com/pascalvanderheiden/turbo-agent-skills-cli/issues/1) is unaddressed).
- **No cross-skill consistency/conflict checking.** No command inspects multiple installed skills for duplication, overlapping triggers, or contradictory instructions (the Kratos-referenced Consistency Check capability is unaddressed).
- **No cross-assistant normalization beyond placement.** `--agent` only changes *where* a skill is copied; it does not adapt or rewrite skill content for host-specific capabilities beyond the file-placement convention itself.
- **No local inventory intelligence beyond flat listing.** `gh skill list` enumerates installed skills with scope/version/pin metadata but does not report cost proxies (token/context budget), duplicate/overlap detection, or usage history.

---

## 2. agentskills.io — the specification `gh skill` targets (primary source, referenced by `gh`)

`gh skill install --help` and `gh skill publish --help` both point directly at `https://agentskills.io/specification` as *the* Agent Skills format definition `gh` validates against. Fetched directly:

- A skill is a directory with a required `SKILL.md` plus optional `scripts/`, `references/`, `assets/`.
- `SKILL.md` frontmatter fields: `name` (required, ≤64 chars, lowercase/digits/hyphens, no leading/trailing/double hyphen, must match directory name), `description` (required, ≤1024 chars), `license` (optional), `compatibility` (optional, ≤500 chars), `metadata` (optional, free-form string map), `allowed-tools` (optional, experimental, space-separated tool list).
- **Progressive disclosure** model: metadata (~100 tokens) always resident, full body (<5000 tokens recommended) loaded only on activation, `scripts/`/`references/`/`assets/` loaded on demand — this is the authoritative source for the "resident vs. body cost" distinction that `asm` operationalizes into `asm stats --tokens` (§3).
- A companion validator, `skills-ref`, is referenced (`skills-ref validate ./my-skill`) and lives in [agentskills/agentskills:skills-ref](https://github.com/agentskills/agentskills/tree/main/skills-ref) inside the `agentskills/agentskills` "Specification and documentation for Agent Skills" repository (confirmed via GitHub API: top-level contents include `skills-ref/`, `docs/`, `CONTRIBUTING.md`).

**Important naming disambiguation for the ticket:** the ticket names `agentskills.in`; the canonical spec that `gh` cites is hosted at **`agentskills.io`** (different TLD/owner). `agentskills.in` (fetched as `https://www.agentskills.in/`) is a *separate*, unrelated "package manager for AI agent skills" marketing site/CLI product ("Like npm, but for AI coding assistants," `npx agent-skills-cli search react`), not the spec authority. Both are treated here per the map's framing: `agentskills.io`'s specification is a primary normative source (it is what `gh skill publish` validates against), while `agentskills.in`'s product is an example only, consistent with the parent map's note that "agentskills.in and asm are reference products, not requirements" ([#1](https://github.com/pascalvanderheiden/turbo-agent-skills-cli/issues/1)).

---

## 3. agentskills.in and asm — examples, not authorities

### 3.1 agentskills.in (`www.agentskills.in`)

Self-description fetched directly from the live site: "The Package Manager for AI Agent Skills — Install, share, and discover skills for Claude, Cursor, Copilot, and more. Like npm, but for AI coding assistants," distributed as `npx agent-skills-cli`. The public docs page is minimal (a single `npx` quick-start line was all that resolved at `/docs`). Treated per the map's instruction as an example product shape, not a specification — no further normative weight is given to it here beyond the ideas list below.

### 3.2 `github.com/luongnv89/asm` ("agent-skill-manager")

Fetched directly from the repository README (MIT-licensed, npm package `agent-skill-manager`, Node.js 22+, 21 supported agent "providers," catalog claims of 6,905 skills across 85 repos).

**Capability surface (from README command tables):**

| Command | Purpose |
|---|---|
| `asm list [--json]` | Cross-provider inventory: every installed skill across every supported agent/scope in one view |
| `asm search "<query>" [--json] [--tag]` | Search installed + catalog skills, with local tag filtering |
| `asm install <name\|github:owner/repo> [--yes] [--json] [-p <provider>]` | Install with frontmatter validation, pre-install security scan, pinned registry commits |
| `asm inspect <skill> [--json]` | Skill metadata dump |
| `asm audit [--yes]` | Removes redundant/duplicate skill installs |
| `asm audit overlap` | Finds **semantically** overlapping skills with different names doing the same job |
| `asm audit security [github:owner/repo]` | Pre-install scan for shell exec, network access, credential exposure |
| `asm audit residency` | Ranks installed skills by whether their always-resident frontmatter description is "earning" its context cost, and recommends a demotion action per skill |
| `asm stats --tokens [--json]` | Reports resident-description token cost vs. full-body token cost, per tool and per scope |
| `asm get <skill> [--json] [--path]` | Prints a skill's `SKILL.md` body to stdout (or borrows the full directory with `--path`) **without installing/retaining** it — a "reference tier" beneath install |
| `asm tag add/remove <skill> <tag>` | Local, asm-only tagging layer over installed skills |
| `asm bundle install <bundle> [--yes]` | Installs a curated multi-skill bundle in one pass |
| `asm init`, `asm link`, `asm eval`, `asm publish` | Author-side pipeline: scaffold, live-dev symlink, evaluate, publish to the asm registry |
| `asm install --library` / `asm activate` / `asm deactivate` / `asm disable` / `asm enable` | A four-tier "demotion ladder" distinguishing Installed (always resident) → Saved/library (adapt, no trigger) → Disabled (kept on disk, inert) → Reference (`get`, no retained copy) |

**Reusable ideas surfaced in the README itself (not proven to be unique, but concretely demonstrated):**
1. **Attention-budget accounting** — treating "resident" frontmatter tokens (always in every prompt) as a distinct, metered cost from "body" tokens (paid only on activation), directly operationalizing the `agentskills.io` progressive-disclosure model (§2) into a reportable number.
2. **A residency/demotion ladder** with reversible states (installed → library/saved → disabled → reference-only) rather than a binary installed/not-installed model.
3. **A "reference tier" (`asm get`)** — fetch-and-print a skill body with zero retained footprint, explicitly distinct from install, letting an agent "borrow" a skill for one task.
4. **Overlap/duplicate auditing** as a first-class command (`audit`, `audit overlap`) rather than leaving duplication to be discovered by the user.
5. **Security pre-scan** gating both `install` and `get` for shell-exec/network/credential-exposure patterns, with the verdict reported (not silently blocking).
6. **Local tagging** layered on top of installed skills, independent of upstream metadata.
7. **Cross-tool linking/symlinking** so one canonical skill copy can serve multiple agent hosts instead of N independent file copies.

Caveat: this is vendor-authored README prose, not independently verified behavior (no source-code audit was performed here, consistent with "treat asm as an example, not an authority"); claims above are reported as asm's *stated* capability, not validated against its implementation.

---

## 4. Common GitHub repository conventions for the Agent Skill lifecycle

Collected from the primary sources above (not from secondary blog commentary):

- **Repository-as-catalog:** both `gh skill` and the `agentskills.io` spec treat a plain GitHub repository containing `skills/*/SKILL.md` (or nested/scoped variants) as the unit of discovery and distribution — no special registry server is required. `gh skill search` reuses GitHub Code Search itself as the index.
- **Git tags/releases as the versioning plane:** `gh skill install`/`update` resolve versions through git tags, the default branch HEAD, or pinned commit SHAs; `gh skill publish` formalizes this by creating an actual GitHub Release with semver-recommended tags and auto-generated notes, and by prompting to add an `agent-skills` **repository topic** for discoverability.
- **Frontmatter as the sole structured metadata channel:** `name`, `description`, `license`, `compatibility`, `metadata`, `allowed-tools` — all the lifecycle tooling surveyed (gh skill, the spec, asm) reads/writes exclusively through this YAML frontmatter block, with installer-injected provenance (`metadata.github-*`) stripped again at publish time so published skills stay installer-agnostic.
- **Scope convention (project vs. user):** `gh skill install --scope {project|user}` (project = inside the current git repo; user = home directory, available everywhere) is the only scope model surfaced in `gh`'s primary docs; `asm` mirrors this with its own provider-specific project/global scopes.
- **Hidden-directory opt-in:** dot-prefixed skill directories (`.claude/skills/`, `.agents/skills/`, `.github/skills/`) are a known but non-default convention, requiring explicit flags in `gh skill` rather than being auto-discovered.
- **Validation before publish, not after:** `gh skill publish --dry-run`/`--fix` and `agentskills.io`'s `skills-ref validate` both separate "is this a well-formed skill" from "is this installed/working," i.e., static spec conformance is checked independently of runtime behavior.

---

## 5. Comparison matrix — delegate to `gh`, reusable idea, or TASC-owned gap

| Capability area | `gh skill` today | agentskills.io spec | agentskills.in (example) | asm (example) | **TASC disposition** |
|---|---|---|---|---|---|
| Repo-based skill search (GitHub Code Search) | ✅ `gh skill search` | — | has own search/registry | has own catalog search (6,905 skills, 85 repos) | **(a) Delegate to `gh`** — do not reimplement GitHub-wide code search; shell out to `gh skill search` or its API |
| Preview before install | ✅ `gh skill preview` | — | — | implied via `inspect`/`get` | **(a) Delegate to `gh`** for GitHub-hosted previews |
| Install to agent-specific directory, multi-host | ✅ `gh skill install --agent/--scope/--dir` (37 hosts) | defines directory/SKILL.md convention only | install flow | `asm install -p <provider>` (21 providers) | **(a) Delegate to `gh`** for the GitHub-repo install path and host directory placement; GitHub Copilot is TASC's only v1 target per map [#1](https://github.com/pascalvanderheiden/turbo-agent-skills-cli/issues/1), so TASC needs no independent multi-host matrix in v1 |
| Pin/version resolution (tag → branch HEAD; `@version`/`--pin`) | ✅ `gh skill install/update` | defines tags/commit SHA as version plane | — | registry-pinned commits | **(a) Delegate to `gh`** |
| Update / drift detection by tree SHA | ✅ `gh skill update [--dry-run\|--force\|--unpin]` | — | — | — | **(a) Delegate to `gh`** for GitHub-sourced skills; TASC's shared core still needs its own local installed-state model (map [#7](https://github.com/pascalvanderheiden/turbo-agent-skills-cli/issues/7)) to track non-`gh`-installed or local skills consistently |
| Validate/publish against spec | ✅ `gh skill publish --dry-run/--fix/--tag` | authoritative spec + `skills-ref` validator | — | `asm init/link/eval/publish` | **(a) Delegate to `gh`** for the actual GitHub release/topic/publish mechanics; TASC can still run/reuse `skills-ref`-style static validation as a pre-check within its own Consistency Check workflow |
| Flat installed-skill inventory | ✅ `gh skill list --json` (scope/version/pin/sourceURL) | — | — | `asm list --json` | **(a) Delegate listing of `gh`-managed installs to `gh skill list`**, but **(c) TASC-owned**: a canonical cross-source local state model that also covers skills `gh` didn't install |
| Progressive-disclosure / resident-vs-body cost accounting | — | defines the 3-tier loading model (metadata/body/resources) that makes this measurable | — | ✅ `asm stats --tokens`, `audit residency` | **(b) Reusable idea** — adopt attention-budget accounting (resident description tokens vs. body tokens) as a TASC-native metric, grounded in the `agentskills.io` progressive-disclosure spec, not duplicated from `gh` |
| Duplicate / semantic-overlap detection across installed skills | — | — | — | ✅ `asm audit`, `asm audit overlap` | **(b) Reusable idea** feeding into **(c) TASC-owned**: this is squarely the "Consistency Check" capability from map [#1](https://github.com/pascalvanderheiden/turbo-agent-skills-cli/issues/1)/[#4](https://github.com/pascalvanderheiden/turbo-agent-skills-cli/issues/4) — `gh skill` has nothing here; TASC should treat multi-skill conflict/overlap analysis as differentiated product surface |
| Security pre-install scanning | — | — | — | ✅ `asm audit security`, gates `install`/`get` | **(b) Reusable idea** — worth evaluating as a TASC pre-install check, scoped to the local-first trust boundary from map [#1](https://github.com/pascalvanderheiden/turbo-agent-skills-cli/issues/1) (no content leaves the machine) |
| Reference/borrow tier (use without installing) | — | progressive disclosure implies on-demand loading | — | ✅ `asm get [--path]` | **(b) Reusable idea** — a "preview/borrow for one task without persisting install state" mode complements `gh skill preview` (which also doesn't install) but adds the zero-residency framing explicitly |
| Demotion ladder (installed→saved→disabled→reference) | — | — | — | ✅ asm's 4-tier model | **(b) Reusable idea** for TASC's local state model (map [#7](https://github.com/pascalvanderheiden/turbo-agent-skills-cli/issues/7)) — richer than `gh skill`'s binary installed/not-installed |
| **Watchlists** (follow a skill/repo without installing; get notified of new releases/changes) | ❌ absent — confirmed by help text and source package listing (`install/list/preview/publish/search/update` only, no `watch`) | not a spec concern (spec defines file format, not lifecycle tooling) | not evidenced in fetched pages | not evidenced in README (no `asm watch`/follow command found) | **(c) TASC-owned gap** — no surveyed tool exposes a watch/follow primitive; this is explicit v1 differentiated scope per map [#1](https://github.com/pascalvanderheiden/turbo-agent-skills-cli/issues/1) ("install, share, manage, watch, update...") |
| Evaluation (does a skill work / quality scoring) | ❌ absent | ❌ out of scope (format only) | not evidenced | `asm eval` exists as an authoring command but no evaluation methodology/criteria were found in the fetched README | **(c) TASC-owned** — explicitly the Waza-based Evaluation foundation (map [#1](https://github.com/pascalvanderheiden/turbo-agent-skills-cli/issues/1), research issue [#4](https://github.com/pascalvanderheiden/turbo-agent-skills-cli/issues/4)); no primary source reviewed here defines this |
| Cross-assistant adapter logic beyond file placement | partial — `--agent` controls destination directory only | defines format, agent-agnostic by design | unclear from fetched pages | `-p <provider>` controls destination + "cross-tool linking" | **(c) TASC-owned** — v1 scope is GitHub Copilot only (map [#1](https://github.com/pascalvanderheiden/turbo-agent-skills-cli/issues/1)); adapter contract for other assistants is explicitly "not yet specified" |
| Shared typed core driving both CLI and agent harness | n/a (gh is CLI-only) | n/a | n/a (npm CLI only) | n/a (Node CLI/TUI only) | **(c) TASC-owned** — no surveyed tool exposes an agent-harness-integrated core; this is TASC's architectural differentiator per map [#1](https://github.com/pascalvanderheiden/turbo-agent-skills-cli/issues/1) |
| GHEC / data-residency host support | ✅ v2.92.0+ | n/a | not evidenced | not evidenced | **(a) Delegate to `gh`** for enterprise host handling |

---

## 6. Direct answers to the ticket question

1. **Lifecycle capabilities already covered by `gh skill`:** search, preview, install (multi-agent, multi-scope, pinned/versioned), list, update (SHA-diff-based), and publish/validate — all scoped to GitHub-repository-hosted skills, all public preview and explicitly "subject to change without notice" (`gh skill --help`).
2. **Capabilities covered by agentskills.in / asm but not `gh skill`:** attention-budget/token accounting, duplicate/overlap auditing, security pre-scanning, a reference/borrow tier, a multi-stage demotion ladder, and local tagging — all demonstrated by `asm`'s README as an example product, none backed by `gh` or the `agentskills.io` spec.
3. **Capabilities common GitHub conventions already standardize:** repo-as-catalog via `skills/*/SKILL.md` + topics, git tags/releases as the versioning plane, and YAML frontmatter as the only structured metadata channel — all defined by the `agentskills.io` spec and operationalized identically by `gh skill`.
4. **Where no surveyed source has anything:** a watchlist/follow primitive (track without installing, get notified of upstream changes), a working Evaluation methodology, cross-skill Consistency Checks as a first-class workflow (only asm's `audit overlap` is a loose analogue), and any shared-core architecture spanning a CLI and an agent harness. These four are TASC's clearest differentiated product boundary, consistent with map [#1](https://github.com/pascalvanderheiden/turbo-agent-skills-cli/issues/1)'s stated scope ("install, share, manage, watch, update, discover, search, evaluate, improve, and check consistency") and explicitly *not* covered by delegating to `gh`.

---

## Sources consulted (primary)

- `gh skill --help`, `gh skill {install,list,preview,publish,search,update} --help` — captured live from `gh version 2.101.0 (2026-09-15)` installed in the research worktree.
- `cli/cli:pkg/cmd/skills/skills.go` — [github.com/cli/cli](https://github.com/cli/cli/blob/trunk/pkg/cmd/skills/skills.go) (fetched via GitHub Contents API), confirming the exact six-subcommand registration and the absence of any watch/evaluate/consistency command.
- `cli/cli` release notes v2.90.0 through v2.102.0 — [github.com/cli/cli/releases](https://github.com/cli/cli/releases), fetched via GitHub Releases API, filtered to entries mentioning "skill".
- GitHub Blog changelog: [Manage agent skills with GitHub CLI, 2026-04-16](https://github.blog/changelog/2026-04-16-manage-agent-skills-with-github-cli/).
- Agent Skills specification: [agentskills.io/specification](https://agentskills.io/specification).
- Agent Skills spec repository: [github.com/agentskills/agentskills](https://github.com/agentskills/agentskills) (contents listing via GitHub API, confirming `skills-ref/` validator).
- [agentskills.in](https://agentskills.in) / [www.agentskills.in](https://www.agentskills.in/) and its `/docs` page — fetched directly, treated as example product only.
- [github.com/luongnv89/asm](https://github.com/luongnv89/asm) README — fetched directly, treated as example product only; no source-code audit performed.
- Wayfinder issues: [#1](https://github.com/pascalvanderheiden/turbo-agent-skills-cli/issues/1) (map), [#2](https://github.com/pascalvanderheiden/turbo-agent-skills-cli/issues/2), [#3](https://github.com/pascalvanderheiden/turbo-agent-skills-cli/issues/3), [#4](https://github.com/pascalvanderheiden/turbo-agent-skills-cli/issues/4), [#5](https://github.com/pascalvanderheiden/turbo-agent-skills-cli/issues/5), [#6](https://github.com/pascalvanderheiden/turbo-agent-skills-cli/issues/6), [#7](https://github.com/pascalvanderheiden/turbo-agent-skills-cli/issues/7) — read for scope/boundary context only, not edited.

## Gaps and uncertainties

- `asm`'s claims (`audit overlap`, `audit security`, `asm eval`, catalog size "6,905 skills from 85 repos") are reported as README-stated capability only; no source-code or executable verification was performed, consistent with treating it as an example, not an authority.
- `agentskills.in`'s own feature set beyond its one-line homepage and `/docs` quick-start could not be fully enumerated — its documentation site returned minimal content from the pages fetched (`/` and `/docs`); deeper features (if any) were not reachable within this research pass. If more detail is needed, the next step would be to run `npx agent-skills-cli --help` directly rather than scraping the marketing site further.
- No GitHub-first-party documentation page (docs.github.com) specifically for "Agent Skills" repository conventions was located beyond the `agentskills.io` spec and the `gh` CLI manual itself; conventions in §4 are inferred from `gh skill`'s and the spec's combined behavior, which is the strongest primary-source signal available.
- This research did not execute `gh skill` network commands (`search`, `install` against live repos) to avoid mutating the isolated worktree or hitting rate limits; all behavioral claims come from `--help` text and source code, which is authoritative for *documented* behavior but was not runtime-verified end-to-end.
