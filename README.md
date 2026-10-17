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
```

`skill list` discovers skills with a `SKILL.md` from `.github/skills` in the
current directory and `~/.copilot/skills`. Local State is stored at
`$XDG_STATE_HOME/tasc/state.json` or `~/.local/state/tasc/state.json`; set
`TASC_STATE_PATH` to use another file.

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
