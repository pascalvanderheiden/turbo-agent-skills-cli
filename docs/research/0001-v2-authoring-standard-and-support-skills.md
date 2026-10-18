# Research: Agent Skills authoring standard & packaging grilling/skill-creator guidance

Research for issue [#41](https://github.com/pascalvanderheiden/turbo-agent-skills-cli/issues/41) (child of map [#39](https://github.com/pascalvanderheiden/turbo-agent-skills-cli/issues/39), blocking [#50](https://github.com/pascalvanderheiden/turbo-agent-skills-cli/issues/50)).

> No existing `docs/research/` convention was found in this repo (only `docs/agents/`, `docs/prototypes/`, `docs/site/`). This file establishes one, numbered like `docs/adr/` per `docs/agents/domain.md`.

## Question

What exact authoring practices from agentskills.io should TASC enforce, and what are the viable ways to package or invoke `grilling` and `skill-creator` guidance without making TASC depend on a user's preinstalled personal skills?

## 1. The normative Agent Skills format (agentskills.io)

Source: [Specification](https://agentskills.io/specification.md) (community-governed continuation of Anthropic's original spec; code Apache-2.0, docs CC-BY-4.0 per [agentskills/agentskills README](https://github.com/agentskills/agentskills)).

### MUST-level (structural/validation requirements)

- A skill is a directory whose only required member is `SKILL.md`, with optional `scripts/`, `references/`, `assets/`, and arbitrary other files.
- `SKILL.md` **must** contain YAML frontmatter followed by a Markdown body.
- `name` (**required**): 1–64 chars; lowercase unicode alphanumerics and hyphens only; no leading/trailing hyphen; no consecutive hyphens; **must match the parent directory name**.
- `description` (**required**): 1–1024 chars, non-empty.
- `license`, `compatibility` (≤500 chars), `metadata` (string→string map), `allowed-tools` (space-separated, experimental) are all **optional** frontmatter fields with no body-content requirement.
- The reference validator is `skills-ref validate` from [agentskills/agentskills `skills-ref`](https://github.com/agentskills/agentskills/tree/main/skills-ref) (Apache-2.0; explicitly "for demonstration purposes only, not meant for production" — TASC should not shell out to it as-is but can mirror its rule set, e.g. via its `validate`/`read_properties` functions as a reference algorithm).

### SHOULD-level (recommended, not enforced by the spec)

- Body: "no format restrictions," but recommended sections are step-by-step instructions, input/output examples, and edge cases.
- Keep `SKILL.md` **under 500 lines / ~5,000 tokens**; move detail into `references/*.md`, loaded on demand (progressive disclosure, 3 tiers: ~100-token catalog entry → <5000-token body on activation → scripts/references/assets loaded only when referenced).
- File references should be **relative paths from the skill root**, and reference chains should stay **one level deep**.
- `description` should use imperative phrasing ("Use this skill when…"), name concrete trigger phrases/contexts, and stay concise — this is the *sole* triggering signal before activation ([Optimizing descriptions](https://agentskills.io/skill-creation/optimizing-descriptions.md)).
- Scripts should pin versions (`npx eslint@9`, `uv run` with PEP 723 inline deps, etc.), be self-contained, and document prerequisites in `compatibility` rather than assuming them ([Using scripts](https://agentskills.io/skill-creation/using-scripts.md)).
- Authoring process guidance ([Best practices](https://agentskills.io/skill-creation/best-practices.md)): ground skills in real task transcripts or existing artifacts (not generic LLM genericity); omit what the agent already knows; scope each skill as one coherent unit of work; match instruction prescriptiveness to task fragility; provide a default tool/approach rather than a menu; prefer reusable *procedures* over one-off *answers*; iterate skills against real execution traces, not just first drafts.

### Client/runtime conventions (not part of `SKILL.md` itself, but relevant to how TASC discovers/installs skills)

Source: [Adding skills support to your agent](https://agentskills.io/client-implementation/adding-skills-support.md).

- Scan scopes: project (`<project>/.<client>/skills/`, `<project>/.agents/skills/`) and user (`~/.<client>/skills/`, `~/.agents/skills/`). `.agents/skills/` is the emerging cross-client convention; `.claude/skills/` is kept "for pragmatic compatibility."
- Collision rule: **project-level overrides user-level**; same-scope collisions just need a consistent, logged tie-break.
- **Trust gating**: project-level skills from an untrusted/freshly cloned repo should require explicit trust before being loaded, since they can silently inject instructions into agent context — directly relevant to any TASC feature that treats repository-bundled skills as discoverable.
- Malformed YAML (e.g., unquoted colons in `description`) is common across clients; a tolerant parser with a quoting/escaping fallback is recommended.

## 2. What TASC's locally installed support skills actually are

Inspected at `/Users/pascalvanderheiden/.copilot/skills/{grilling,skill-creator}` (this machine, Copilot CLI's user-level skills directory).

- `grilling`: present. Contents are exactly `SKILL.md` (no frontmatter `license` field, no bundled `LICENSE`) plus `agents/openai.yaml` (a display-name/short-description override, not part of the agentskills.io spec). **No license or provenance metadata ships with the installed copy.**
- `skill-creator`: **not present** at that path on this machine (`ls` returns "No such file or directory"; no match under `~/.copilot/skills` by name search). It is only reachable through this runtime's `skill` tool-search mechanism, which does not correspond to a local, inspectable file the way `grilling` does. TASC cannot assume any user has this installed, and cannot treat "install path" as portable — confirmed empirically, not just architecturally.

### Provenance tracing

- `grilling`'s installed body is a **near-verbatim, slightly stale copy** of `skills/productivity/grilling/SKILL.md` from **mattpocock/skills** ("Skills For Real Engineers", MIT-licensed — `Copyright (c) 2026 Matt Pocock`, see [LICENSE](https://raw.githubusercontent.com/mattpocock/skills/main/LICENSE)). Diff: the upstream file has one extra sentence ("Word each question so 'yes' accepts your recommended answer.") absent from the locally installed copy — i.e., the installed copy has already drifted from upstream and carries **no copy of the MIT notice**, which the MIT license requires to be retained in copies/substantial portions. That repo is distributed to agents via `npx skills@latest add mattpocock/skills`, Claude Code/Copilot CLI plugin marketplaces, or Gemini CLI's `gemini skills install` (see repo README), none of which are guaranteed present or network-reachable in a TASC run.
- `skill-creator` (the well-known meta-skill for drafting/evaluating skills) is **not** in mattpocock/skills (`gh api search/code -f q='"skill-creator" repo:mattpocock/skills'` → 0 results). It is Anthropic's own example skill at [anthropics/skills `skills/skill-creator`](https://github.com/anthropics/skills/tree/main/skills/skill-creator), which **ships its own `LICENSE.txt` (Apache License 2.0) inside the skill folder itself** — i.e., the canonical skill-creator is self-licensing at the directory level, *not* via the spec's optional `license:` frontmatter key (its `SKILL.md` frontmatter has only `name`/`description`, no `license` field, confirmed by reading the raw file). The [anthropics/skills README](https://github.com/anthropics/skills) states "Many skills in this repo are open source (Apache 2.0)" while the `docx`/`pdf`/`pptx`/`xlsx` document skills are explicitly "source-available, not open source" — license status is **per-skill**, not blanket for the repo, so TASC must check per-directory, not assume repo-wide terms.

## 3. Options for TASC to use this guidance without depending on a user's personal install

The parent map (#39, "Not yet specified") already flags this exact gap: *"Exact guided authoring playbook and how TASC packages or invokes support skills such as grilling and skill-creator."* Candidate approaches, in order of how directly they touch licensed third-party text:

1. **Re-implement the behavior natively, don't vendor the text.** `grilling`'s interview loop (frontier/rounds of settled-vs-open decisions) is a *pattern*, not a large body of irreplaceable prose (~250 words). TASC's own "Skill Authoring" contract (per `CONTEXT.md`'s `Skill Draft`/`Skill Workspace` vocabulary and issue #50) can implement an equivalent interview loop as original TASC product copy, citing mattpocock/skills as prior art in developer docs rather than redistributing its MIT text. This sidesteps attribution/update bookkeeping entirely and removes the runtime dependency on any external skill. **Tradeoff:** behavior can silently diverge from the upstream skill's intent over time without someone re-reading it.
2. **Vendor a pinned snapshot with attribution preserved.** Copy the exact files TASC needs into the TASC repo/binary, pinned to a source commit SHA (TASC's own `Source Revision` concept from `CONTEXT.md` already models this), and keep the license text alongside:
   - `skill-creator`: copy is straightforward — it already carries its own `LICENSE.txt` (Apache-2.0); vendoring it plus that file is self-documenting and Apache-2.0 permits redistribution with notice.
   - `grilling`: MIT requires the copyright/license notice to travel with the copy; since mattpocock/skills' top-level `LICENSE` covers the whole repo rather than each skill folder individually, TASC would need to carry that root `LICENSE` file (or an equivalent attribution block) alongside any vendored `grilling` content.
   - **Tradeoff:** deterministic and offline-safe (content is byte-fixed at build time), but requires a manual/periodic re-pull step to pick up upstream fixes, and bakes a specific revision into TASC's own release cadence.
3. **Fetch-on-demand from the pinned upstream repo at runtime**, verifying a content digest before use (mirrors `CONTEXT.md`'s `Source Revision` "immutable commit or digest, falling back to content digest"). **Tradeoff:** always current, but trades away offline/deterministic behavior and requires network + GitHub availability during an authoring session; also reopens the exact "preinstalled personal skill" trust/version problem the issue is trying to avoid, just moved to TASC's own source pin instead of the user's.
4. **Treat a user's existing local install as an optional enhancement only**, never a requirement: if TASC detects a locally installed `grilling`/`skill-creator` (matching the install-location conventions above), it may *offer* to use it, but the authoring flow must have a self-sufficient fallback (per options 1 or 2) so results don't vary by whether a given user happens to have these personal skills installed. This directly matches the issue's ask ("without making TASC depend on a user's preinstalled personal skills") and is compatible with any of 1–3 as the fallback.
5. **Invoke via an external package manager at runtime** (`npx skills@latest add mattpocock/skills`, Codex/Copilot CLI plugin marketplaces) instead of a direct fetch. **Tradeoff:** same non-determinism/network issues as option 3, plus a new third-party CLI dependency (`skills@latest`) whose own versioning/behavior TASC doesn't control.

### Recommendation implied by the evidence

Given that (a) the installed `grilling` copy on this machine is already stale and missing required MIT attribution, (b) `skill-creator` isn't even present locally despite being "available," and (c) per-skill licensing varies even within one upstream repo — **TASC should not read or depend on a user's live `~/.copilot/skills/*` (or any other client's) directory as an authoring-guidance source.** The most robust combination is **Option 1 (re-implement the authoring interview as original TASC behavior, validated against the agentskills.io spec in Section 1) for the "grilling-style" discovery step**, plus **Option 2 (vendor a pinned, attributed snapshot) only if/when TASC wants to reuse Anthropic's actual `skill-creator` workflow content**, since it is the one of the two that is self-licensing and Apache-2.0 (permissive, attribution-only). Option 4 can still be layered on top so a user's own installed skills are *detected and offered*, never silently assumed.

## 4. Licensing/provenance summary table

| Skill | Upstream source | License | Attribution present locally? |
|---|---|---|---|
| `grilling` (installed) | [mattpocock/skills](https://github.com/mattpocock/skills), `skills/productivity/grilling/SKILL.md` | MIT (repo-level `LICENSE`) | **No** — no license file or notice ships with the installed copy; frontmatter has no `license` field |
| `skill-creator` (not installed here) | [anthropics/skills](https://github.com/anthropics/skills), `skills/skill-creator/` | Apache-2.0, via a `LICENSE.txt` **inside the skill folder** | N/A — not present on this machine to check |
| agentskills.io spec text | [agentskills/agentskills](https://github.com/agentskills/agentskills) | Code: Apache-2.0; docs: CC-BY-4.0 | N/A (web source, not a local skill) |

## Sources

- https://agentskills.io/specification.md
- https://agentskills.io/skill-creation/best-practices.md
- https://agentskills.io/skill-creation/optimizing-descriptions.md
- https://agentskills.io/skill-creation/using-scripts.md
- https://agentskills.io/skill-creation/quickstart.md
- https://agentskills.io/client-implementation/adding-skills-support.md
- https://github.com/agentskills/agentskills (root README: license split, Apache-2.0 code / CC-BY-4.0 docs)
- https://github.com/agentskills/agentskills/tree/main/skills-ref (reference validator, Apache-2.0, "demonstration purposes only")
- https://github.com/mattpocock/skills (README: install mechanisms, newsletter attribution)
- https://raw.githubusercontent.com/mattpocock/skills/main/LICENSE (MIT, Copyright (c) 2026 Matt Pocock)
- https://raw.githubusercontent.com/mattpocock/skills/main/skills/productivity/grilling/SKILL.md (upstream text, diffed against local copy)
- https://github.com/anthropics/skills (README: per-skill license split; document skills are "source-available, not open source")
- https://raw.githubusercontent.com/anthropics/skills/main/skills/skill-creator/SKILL.md (frontmatter has no `license` key)
- https://raw.githubusercontent.com/anthropics/skills/main/skills/skill-creator/LICENSE.txt (Apache-2.0, bundled in-folder)
- Local inspection: `/Users/pascalvanderheiden/.copilot/skills/grilling/SKILL.md`, `/Users/pascalvanderheiden/.copilot/skills/grilling/agents/openai.yaml` (this machine, read 2026-10-18)
- Local inspection: `/Users/pascalvanderheiden/.copilot/skills/skill-creator` — confirmed absent on this machine (`ls` error, no filename match under `~/.copilot/skills`)
- Local inspection: `/Users/pascalvanderheiden/.copilot/skills/research/SKILL.md` — this repo's own research-note convention, which this document follows
- Repo-relative: `CONTEXT.md` (TASC domain vocabulary: Skill Authoring, Skill Draft, Skill Workspace, Source Revision)
- Repo-relative: `docs/agents/domain.md` (ADR numbering convention `NNNN-slug.md`, reused here for `docs/research/`)
- GitHub issues: [#39](https://github.com/pascalvanderheiden/turbo-agent-skills-cli/issues/39) (parent map), [#41](https://github.com/pascalvanderheiden/turbo-agent-skills-cli/issues/41) (this question), [#50](https://github.com/pascalvanderheiden/turbo-agent-skills-cli/issues/50) (blocked-by #41, defines the Skill Authoring contract this research feeds)
