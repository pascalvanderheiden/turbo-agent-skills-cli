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

tasc status
tasc insights
tasc evaluate
```

## Command surface

```bash
tasc status
# Show tracked sources and installed skill inventory

tasc add-source my-org/my-skill https://github.com/my-org/my-skill
tasc insights
# Show prioritized recommendations for the local skill environment

tasc evaluate
# Run the local evaluation pass for tracked skills
```

## Architecture

- `tasc-core` holds the canonical language model and state types
- `tasc-agent` exposes the Copilot-agent service layer
- `tasc-cli` provides the command-line surface

## Release and delivery

The repo is set up for GitHub Releases and Pages delivery patterns, with a modern CI build pipeline for macOS, Linux, and Windows. The distribution model intentionally favors immutable binaries and provenance over native package-manager shortcuts in v1.

## Related work

- GitHub CLI skill lifecycle: delegated to `gh skill`
- Waza: evaluation foundation
- Kratos patterns: used as reference for consistency checks and repair guidance
