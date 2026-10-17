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

## Architecture

- `tasc-core` holds the canonical language model and state types
- `tasc-agent` provides the shared inventory, adoption, adapter, and state service
- `tasc-cli` provides the command-line surface

## Release and delivery

The repo is set up for GitHub Releases and Pages delivery patterns, with a modern CI build pipeline for macOS, Linux, and Windows. The distribution model intentionally favors immutable binaries and provenance over native package-manager shortcuts in v1.

## Related work

- GitHub CLI skill lifecycle: delegated to `gh skill`
- Waza: evaluation foundation
- Kratos patterns: used as reference for consistency checks and repair guidance
