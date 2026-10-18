# Research: marketplace and plugin provenance detection

Research for [#43](https://github.com/pascalvanderheiden/turbo-agent-skills-cli/issues/43),
captured 2026-10-18. TASC's v1 scope is GitHub Copilot only; cross-assistant
support is explicit later roadmap
([issue #1](https://github.com/pascalvanderheiden/turbo-agent-skills-cli/issues/1)).
This research therefore grounds GitHub Copilot provenance first, then checks
it against the other coding-assistant surfaces `gh skill` itself already
targets, so a later Adapter Target can reuse one representation instead of
inventing one per assistant.

## Executive summary

This research verified reliable **local, tool-recorded** provenance for two
first-party install paths, and both are narrower than "every installed skill":

1. **`gh skill install`** (the required GitHub foundation) writes two
   authoritative local artifacts: GitHub-prefixed keys in the SKILL.md
   `metadata` frontmatter map, and a separate `.skill-lock.json` file. Only
   skills installed or updated through `gh skill` carry this evidence.
2. **Claude Code plugins** record installed-plugin identity as
   `<name>@<marketplace>` entries in settings files, plus two JSON files
   under `~/.claude/plugins/`. This only covers Claude Code's own plugin
   system, not skills copied by hand into `.claude/skills/`.

Both schemes degrade the same way: a skill with **no** recorded metadata is
not provably "unknown origin" — it may be a hand-copied skill, a skill
installed by a tool that doesn't write provenance, or a skill whose
provenance fields were stripped or hand-edited. None of the mechanisms below are cryptographically signed; all are plain-text
and editable by anyone with filesystem access. They are authoritative only
about what the named installer or assistant recorded, not about publisher
identity or content authenticity. Confidence must be represented explicitly
rather than inferred from "field is present."

GitHub Copilot's own agent runtime (Copilot SDK) has **no provenance
concept** at all: it only resolves `skillDirectories` into subdirectories and
reads `SKILL.md`. Provenance is entirely a `gh skill` concern.

## Source baseline

Primary sources used, all pinned to a commit:

- GitHub CLI source at commit
  [`fa92838`](https://github.com/cli/cli/tree/fa92838bef95fe4a29c1587693885d5e9bdb6cf1),
  specifically
  [`internal/skills/registry/registry.go`](https://github.com/cli/cli/blob/fa92838bef95fe4a29c1587693885d5e9bdb6cf1/internal/skills/registry/registry.go),
  [`internal/skills/lockfile/lockfile.go`](https://github.com/cli/cli/blob/fa92838bef95fe4a29c1587693885d5e9bdb6cf1/internal/skills/lockfile/lockfile.go),
  [`internal/skills/frontmatter/frontmatter.go`](https://github.com/cli/cli/blob/fa92838bef95fe4a29c1587693885d5e9bdb6cf1/internal/skills/frontmatter/frontmatter.go),
  [`pkg/cmd/skills/install/install.go`](https://github.com/cli/cli/blob/fa92838bef95fe4a29c1587693885d5e9bdb6cf1/pkg/cmd/skills/install/install.go),
  [`pkg/cmd/skills/list/list.go`](https://github.com/cli/cli/blob/fa92838bef95fe4a29c1587693885d5e9bdb6cf1/pkg/cmd/skills/list/list.go), and
  [`pkg/cmd/skills/update/update.go`](https://github.com/cli/cli/blob/fa92838bef95fe4a29c1587693885d5e9bdb6cf1/pkg/cmd/skills/update/update.go).
- GitHub Copilot SDK skills documentation at commit
  [`b04171f`](https://github.com/github/copilot-sdk/blob/b04171f698fd80bfd5988505fd93d4a9b80a3934/docs/features/skills.md)
  (same pin as the prior Copilot SDK research).
- Agent Skills format specification, source
  [`docs/specification.mdx`](https://github.com/agentskills/agentskills/blob/69ef37e9424c0a7ea9dd2293b559e43ec8176379/docs/specification.mdx)
  at commit `69ef37e`, also published at
  [agentskills.io/specification](https://agentskills.io/specification).
- Anthropic's official Claude Code docs:
  [Create a marketplace](https://code.claude.com/docs/en/plugin-marketplaces),
  [Install and manage plugins](https://code.claude.com/docs/en/plugins/install),
  [Plugin manifest reference](https://code.claude.com/docs/en/plugins-reference),
  [Marketplace reference](https://code.claude.com/docs/en/plugins/marketplace-reference), and
  [Plugin loading reference](https://code.claude.com/docs/en/plugins/loading).
  These pages are unversioned/living docs; content reflects the state fetched
  on 2026-10-18.
- `vercel-labs/skills` source at commit
  [`13e4063`](https://github.com/vercel-labs/skills/blob/13e4063a1cf913f5606d57d42ab83a86f5001e04/src/skill-lock.ts)
  and
  [`src/constants.ts`](https://github.com/vercel-labs/skills/blob/13e4063a1cf913f5606d57d42ab83a86f5001e04/src/constants.ts),
  used only to independently confirm the lock-file format `gh skill`
  references (see below), not as a TASC dependency.

## Authoritative evidence: GitHub CLI (`gh skill`)

`gh skill install` writes provenance in two independent places, both only
populated on skills it manages:

### 1. SKILL.md frontmatter `metadata` map (travels with the skill content)

The Agent Skills specification reserves an optional `metadata` field as "a
map from string keys to string values" that "clients can use ... to store
additional properties not defined by the Agent Skills spec," recommending
"reasonably unique" key names to avoid collisions
([specification.mdx](https://github.com/agentskills/agentskills/blob/69ef37e9424c0a7ea9dd2293b559e43ec8176379/docs/specification.mdx)).

`gh skill install` uses exactly that extension point. `InjectGitHubMetadata`
writes `github-repo`, `github-ref`, `github-tree-sha`, `github-path`, and
(when pinned) `github-pinned` into the frontmatter `metadata` map
([`frontmatter.go:70-96`](https://github.com/cli/cli/blob/fa92838bef95fe4a29c1587693885d5e9bdb6cf1/internal/skills/frontmatter/frontmatter.go#L70-L96)).
A separate path, `InjectLocalMetadata`, instead writes a single `local-path`
key and removes any `github-*` keys when the skill came from a local
directory rather than GitHub
([`frontmatter.go:102-123`](https://github.com/cli/cli/blob/fa92838bef95fe4a29c1587693885d5e9bdb6cf1/internal/skills/frontmatter/frontmatter.go#L102-L123)).

`gh skill list` and `gh skill update` both read this same metadata back by
re-parsing the installed `SKILL.md` on disk — not from a side-car index —
via `parseInstalledSkill`, which extracts `github-tree-sha`, `github-pinned`,
and `github-path`, and resolves the owning repo through
`source.ParseMetadataRepo`
([`update.go:601-623`](https://github.com/cli/cli/blob/fa92838bef95fe4a29c1587693885d5e9bdb6cf1/pkg/cmd/skills/update/update.go#L601-L623)).
`gh skill` even uses this metadata to detect **re-published** skills: before
installing, `checkUpstreamProvenance` fetches the SKILL.md at the target
commit, parses its `metadata.github-repo`, and if it differs from the
repository the user is installing from, offers to redirect the install to
the declared upstream, treating the field as a (non-cryptographic) pointer
to the original publisher
([`install.go:1292-1367`](https://github.com/cli/cli/blob/fa92838bef95fe4a29c1587693885d5e9bdb6cf1/pkg/cmd/skills/install/install.go#L1292-L1367)).

Because this metadata uses the spec-reserved extension map and lives in the
file itself, it is the most portable installer-recorded signal: it survives
being copied between machines or committed into a different repository, but
for the same reason it can be hand-edited or omitted, and is **not**
cryptographically bound to the content it describes.

### 2. `~/.agents/.skill-lock.json` (local install ledger)

`gh skill install` separately records every install in a JSON lock file at
`~/.agents/.skill-lock.json` through `lockfile.RecordInstall`, called from
the shared installer
([`installer.go:79`, `installer.go:132`](https://github.com/cli/cli/blob/fa92838bef95fe4a29c1587693885d5e9bdb6cf1/internal/skills/installer/installer.go)).
Each entry records `source` (`owner/repo`), `sourceType` (`"github"`),
`sourceUrl`, `skillPath`, `skillFolderHash` (a GitHub tree SHA),
`installedAt`/`updatedAt` timestamps, and an optional `pinnedRef`
([`lockfile.go:23-31`](https://github.com/cli/cli/blob/fa92838bef95fe4a29c1587693885d5e9bdb6cf1/internal/skills/lockfile/lockfile.go#L23-L31),
[`lockfile.go:97-137`](https://github.com/cli/cli/blob/fa92838bef95fe4a29c1587693885d5e9bdb6cf1/internal/skills/lockfile/lockfile.go#L97-L137)).
A code comment states the schema version must match "Vercel's
`CURRENT_LOCK_VERSION` for interop"
([`lockfile.go:18-20`](https://github.com/cli/cli/blob/fa92838bef95fe4a29c1587693885d5e9bdb6cf1/internal/skills/lockfile/lockfile.go#L18-L20)).
That claim is independently verifiable: `vercel-labs/skills` writes the same
path and version number
([`constants.ts`](https://github.com/vercel-labs/skills/blob/13e4063a1cf913f5606d57d42ab83a86f5001e04/src/constants.ts),
`CURRENT_VERSION = 3`, `~/.agents/.skill-lock.json`;
[`skill-lock.ts:1-35`](https://github.com/vercel-labs/skills/blob/13e4063a1cf913f5606d57d42ab83a86f5001e04/src/skill-lock.ts#L1-L35)),
and additionally records `pluginName` (the plugin a skill was installed as
part of) and `sourceType` values beyond `"github"` (e.g. `"mintlify"`,
`"huggingface"`, `"local"`). This is two independent first-party CLI vendors
converging on one on-disk schema and path — the closest thing to a de facto
cross-tool lock-file standard found in this research, but it is **not** a
published, versioned spec; it is reverse-engineerable interop between two
implementations, so TASC should treat its shape as observed, not contractual.

`gh skill` does **not** read this lock file back for `list`/`update`
(those re-parse frontmatter, per above), so the lock file is best understood
as a side ledger — useful as a secondary signal or for other tools reading
`~/.agents/.skill-lock.json`, but not `gh skill`'s own source of truth.

### 3. `gh skill list --json` as the delegated, typed surface

`gh skill list` exposes the same provenance as structured fields via
`--json skillName,agentHosts,scope,sourceURL,version,pinned,path`
([`list.go:73-103`](https://github.com/cli/cli/blob/fa92838bef95fe4a29c1587693885d5e9bdb6cf1/pkg/cmd/skills/list/list.go#L73-L103)).
This reinforces the existing "delegate to `gh skill`" decision from
[`docs/research/copilot-sdk-gh.md`](https://github.com/pascalvanderheiden/turbo-agent-skills-cli/blob/research/copilot-sdk-gh/docs/research/copilot-sdk-gh.md):
TASC should consume this JSON rather than re-implement frontmatter/lock-file
parsing.

### Copilot SDK has no provenance concept

The Copilot SDK skills doc describes `skillDirectories` purely as a list of
parent directories the runtime scans for immediate subdirectories containing
a `SKILL.md`
([`skills.md`](https://github.com/github/copilot-sdk/blob/b04171f698fd80bfd5988505fd93d4a9b80a3934/docs/features/skills.md)).
It has no install, marketplace, or source-tracking API. Any provenance a
Copilot session can "see" is whatever is already sitting in a loaded
`SKILL.md`'s frontmatter — i.e., exactly the `gh skill`-authored metadata
above, read as plain content, not as a typed/validated contract.

## Authoritative evidence: Claude Code plugins and marketplaces

Claude Code is the other coding assistant `gh skill` already targets as a
first-class `AgentHost`
([`registry.go:49-61`](https://github.com/cli/cli/blob/fa92838bef95fe4a29c1587693885d5e9bdb6cf1/internal/skills/registry/registry.go#L49-L61),
entry `ID: "claude-code"`), so its own first-party provenance model is in
scope even though TASC v1 does not target it.

A Claude Code **marketplace** is a directory/repo with
`.claude-plugin/marketplace.json` declaring a `name`, `owner`, and a
`plugins` array of entries, each with at least `name` and `source`
([Create a marketplace](https://code.claude.com/docs/en/plugin-marketplaces)).
A **plugin** has its own manifest, `.claude-plugin/plugin.json`, which may
declare `repository`, `author`, `homepage`, and a free-form `metadata`
object that "Claude Code doesn't read"
([Plugin manifest reference](https://code.claude.com/docs/en/plugins-reference)) —
i.e. these fields are publisher-asserted and explicitly unvalidated by
Claude Code itself.

What Claude Code *does* record locally, authoritatively, on install:

- **Enablement + identity**: every installed/enabled plugin gets an id of
  the form `<name>@<origin>`, written under the `enabledPlugins` key in one
  of three settings files depending on scope — `~/.claude/settings.json`
  (user), `.claude/settings.json` (project, committed), or
  `.claude/settings.local.json` (local, uncommitted)
  ([Install and manage plugins § Choose an install scope](https://code.claude.com/docs/en/plugins/install)).
  `<origin>` is one of: the registered **marketplace name** (genuine
  marketplace/plugin provenance), `inline` (loaded via `--plugin-dir`/
  `--plugin-url`/env var/SDK option for one session), `skills-dir` (a plugin
  directory dropped straight into `~/.claude/skills/` or
  `.claude/skills/` with no marketplace at all), or `synced` (pushed from
  the user's claude.ai account)
  ([Plugin loading reference § Find where a plugin came from](https://code.claude.com/docs/en/plugins/loading)).
  **This origin tag is the authoritative "was this a marketplace/plugin
  install" signal** — `@skills-dir` and `@inline` are explicit "no, it
  wasn't."
- **Fetched-marketplace ledger**: `~/.claude/plugins/known_marketplaces.json`
  records every marketplace Claude Code has fetched, with its `source`,
  `installLocation`, `lastUpdated`, and `autoUpdate`
  ([Plugin loading reference § Check which stage a plugin reached](https://code.claude.com/docs/en/plugins/loading)).
- **Installed-plugin ledger**: `~/.claude/plugins/installed_plugins.json`
  records each installed plugin's `scope`, `installPath`, and `version`
  (same source).

Claude Code also reserves marketplace names that could otherwise be used to
impersonate Anthropic's own catalog (`claude-plugins-official`,
`anthropic-marketplace`, etc.) and rejects "another spelling" of a reserved
name, which is itself a (non-cryptographic) anti-spoofing control over the
marketplace identity namespace
([Marketplace reference § Reserved names](https://code.claude.com/docs/en/plugins/marketplace-reference)).

### Cross-tool interaction worth flagging

`gh skill install --agent claude-code` installs into `.claude/skills/`
(`ProjectDir: ".claude/skills"` /
`UserDir: ".claude/skills"` in
[`registry.go:52-57`](https://github.com/cli/cli/blob/fa92838bef95fe4a29c1587693885d5e9bdb6cf1/internal/skills/registry/registry.go#L52-L57)),
which is exactly Claude Code's own `skills-dir` auto-load path. A skill installed this way will carry GitHub CLI-recorded `github-*`
frontmatter metadata, but Claude Code itself will record and display it as
`@skills-dir` — i.e. **not** a marketplace/plugin install from Claude Code's
own point of view, even though `gh skill` recorded its upstream repository. A TASC provenance model that asks "is this a
marketplace/plugin skill" as a single yes/no question will get a different
answer depending on which assistant's bookkeeping it asks — this is the
central reason provenance needs to be represented per-adapter-observation,
not collapsed into one boolean.

## Heuristic evidence (not assistant-recorded, inferred by TASC)

These are signals TASC could compute itself, with explicitly bounded
confidence, when no authoritative record exists or to corroborate one:

- **Enclosing Git remote**: if a skill directory sits inside a project's own
  Git working tree, `git remote get-url` / `git log` on that path can show
  which repository and commit currently contain the files, which is
  evidence about *where the files are stored*, not evidence about *how they
  were installed* (hand copy vs. managed install vs. vendored). This is
  TASC-computed, not emitted by any assistant.
- **Directory shape matching a known convention**: presence of a
  `.claude-plugin/marketplace.json` or `.claude-plugin/plugin.json` next to
  a skill is strong evidence the directory *originated* from a Claude Code
  plugin/marketplace layout, even if it was copied in by hand rather than
  installed through `claude plugin install` — i.e. it indicates *shape of
  origin*, not *recorded local installation*.
- **Presence of `github-*` metadata without a corresponding
  `~/.agents/.skill-lock.json` entry** (or vice versa): suggests the skill's
  frontmatter was carried over from a different machine, hand-copied, or
  that the lock file was deleted/never written for that install — a
  consistency signal, not a provenance source on its own.
- **Author-declared manifest fields** (`plugin.json`'s `repository`,
  `author`, `homepage`; frontmatter's own `metadata.author`/`version` from
  the spec's worked example) are self-reported by the skill/plugin publisher
  and explicitly not verified by the assistants that read them. Useful as a
  display hint; must not be treated as equivalent to an assistant-recorded
  install fact.

## Unavailable provenance

Provenance is **not available from any first-party local mechanism** found
in this research for:

- Any skill copied by hand into an assistant's skills directory without
  going through `gh skill install`, `claude plugin install`, or an
  equivalent managed installer (manual copy, template scaffolding, editor
  snippet, etc.).
- Claude Code plugins loaded via `--plugin-dir`/`--plugin-url`/
  `CLAUDE_CODE_PLUGIN_DIRS`/Agent SDK `plugins` option (`@inline` origin) —
  these are deliberately ephemeral and session-scoped, with no persistent
  marketplace record by design.
- Any assistant in `gh skill`'s `Agents` list
  ([`registry.go:49-`](https://github.com/cli/cli/blob/fa92838bef95fe4a29c1587693885d5e9bdb6cf1/internal/skills/registry/registry.go#L49))
  beyond GitHub Copilot and Claude Code: this research did not find each of
  those assistants' own first-party provenance mechanism (e.g. Cursor,
  Codex, Gemini CLI, Cline, and the ~30 others `gh skill` lists only as
  install-path targets). `gh skill`-authored `github-*` frontmatter would
  still be present on disk for skills it installed into those agents'
  directories, but whether each assistant's own runtime records or
  surfaces that as "provenance" is unverified and out of this research's
  bounded scope.
- Content authenticity/integrity in all cases above: no mechanism found is
  cryptographically signed. `github-tree-sha`/`skillFolderHash` detect
  *content drift* against the recorded hash, not *authenticity* of the
  original publisher's identity.

## Proposed assistant-neutral provenance representation

This follows `CONTEXT.md`'s existing vocabulary (`Installation`, `Adapter
Target`, `Source Revision`, `Untracked Installation`) rather than introducing
new terms, and separates *recorded fact* from *derived confidence*. It
proposes shape only — no new product commitment, and no fields invented
beyond what was observed above.

```text
InstallationProvenance (per Installation, i.e. per Adapter Target):
  evidenceKind:        "adapter-recorded" | "content-embedded" | "heuristic" | "unavailable"
  recordedBy:          identifies the mechanism, e.g.
                         "gh-skill-lockfile" | "gh-skill-frontmatter" |
                         "claude-code-enabled-plugins" |
                         "claude-code-installed-plugins-json" | "tasc-git-remote" | ...
  sourceKind:          "marketplace" | "plugin" | "repository" | "local" | "unknown"
  sourceIdentity:      e.g. "owner/repo" (gh), "<plugin>@<marketplace>" (Claude Code)
  sourceRevision:      commit SHA / tree SHA / content hash, when present
  installScope:        "user" | "project" | "local" | "session" (maps to Claude Code's
                        scope and gh skill's Scope)
  observedAt:          timestamp of the local record, not of installation
  selfReported:        true for any field the publisher authored (e.g. plugin.json
                        "repository") rather than the assistant/tool itself
```

Key design implications from the evidence above:

- **Per-Adapter-Target, not per-Skill-Identity.** The Claude Code
  `.claude/skills/` example shows the same files can be "marketplace
  provenance" to one recorder (`gh skill`'s frontmatter) and "no
  marketplace" to another (Claude Code's `@skills-dir`). A single skill can
  have multiple `InstallationProvenance` records, one per Installation.
- **`evidenceKind` must be visible, not just a derived boolean.** Collapsing
  to "has marketplace provenance: yes/no" discards exactly the distinction
  this research needed three sections to make.
- **Self-reported fields are quarantined.** Anything sourced from a
  publisher-authored manifest/frontmatter field that the assistant itself
  does not validate (Claude Code's `plugin.json.repository`; the spec's
  example `metadata.author`) should carry `selfReported: true` and never be
  silently promoted to the same confidence as an assistant-recorded install
  ledger entry.
- **Absence is a first-class state**, matching `Untracked Installation` in
  `CONTEXT.md`: `evidenceKind: "unavailable"` is itself informative (e.g.
  "installed by hand," "installed via a mechanism TASC doesn't recognize")
  and should never be rendered the same as a verified marketplace install.

## Gaps and follow-ups

- Verified in depth: `gh skill` (install/list/update, frontmatter, lock
  file) and Claude Code (marketplaces, plugins, settings scopes, on-disk
  ledgers). Both are primary-sourced from the vendor's own repository or
  docs.
- Not verified, flagged as a gap: first-party provenance mechanisms (if any)
  for the other ~30 assistants `gh skill` already lists as install targets
  (Cursor, Codex, Gemini CLI, Cline, Windsurf-adjacent tools, etc.). A
  follow-up research pass should pick 2-3 of the most relevant to TASC's
  roadmap and repeat this same "find the vendor's own install-time local
  write path" method rather than assuming `gh skill`'s frontmatter
  convention is honored by each target's own tooling.
- Independently confirmed but not contractually guaranteed: the
  `~/.agents/.skill-lock.json` interop between `gh skill` and
  `vercel-labs/skills`. Treat its shape as observed convergence between two
  implementations, not a stable spec TASC can depend on without its own
  capability checks.
- Out of this research's scope, per the parent issue and `CONTEXT.md`:
  designing the actual `Installation`/`Adapter Target` schema, any UI for
  showing provenance (that belongs to
  [#51](https://github.com/pascalvanderheiden/turbo-agent-skills-cli/issues/51)),
  and discovery/sharing semantics (that belongs to
  [#49](https://github.com/pascalvanderheiden/turbo-agent-skills-cli/issues/49)).
