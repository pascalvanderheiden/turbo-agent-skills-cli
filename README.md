# TASC — Turbo Agent Skills CLI

![TASC speed ASCII art banner](docs/site/assets/tasc-speed-banner.svg)

TASC helps you discover, install, create, evaluate, and safely manage coding-agent skills. It keeps inventory and evidence local, makes lifecycle changes explicit, and never silently updates an installed skill.

<!-- pages-content:start -->
# TASC

> Turbo Agent Skills CLI

Local-first tools to discover, install, create, evaluate, and safely manage coding-agent skills.

## Start here

Install TASC 0.1.0 on Linux, macOS, or Windows x64. The installers verify the selected release archive against its published SHA-256 checksum before installing it in a user-writable directory.

```sh
curl -fsSL https://raw.githubusercontent.com/pascalvanderheiden/turbo-agent-skills-cli/v0.1.0/install.sh | bash
```

```powershell
iex (irm https://raw.githubusercontent.com/pascalvanderheiden/turbo-agent-skills-cli/v0.1.0/install.ps1)
```

First run:

```sh
tasc --banner
tasc setup
tasc skill list
tasc state status
```

`tasc --banner` prints the same static banner used at Agent Session startup, as ordinary content without animation or redraw. Piped output, `NO_COLOR`, and `TERM=dumb` use plain ASCII without ANSI escapes. Other commands do not show the banner.

Human Presentation Events share the labels `› Input`, `◆ TASC`, `? Prompt`, `· Info`, `... Working`, `! Warning`, `× Error`, `✓ Done`, and `↳ Next` across the Agent Session, CLI diagnostics, and installers. Installers report `... Detecting`, `... Downloading`, `... Verifying`, and `... Installing`, followed by one `✓ Installed` or `× Error` event; PATH advice uses a separate `↳ Next` row. Capable terminals color only the glyph and label, leaving body text in the terminal foreground. With `NO_COLOR` (including an empty value), piped output, or `TERM=dumb`, events use plain ASCII labels such as `INFO:`, `ERROR:`, and `DONE:` without ANSI, animation, or redraw. Prompts, progress, warnings, and errors use stderr; results use stdout. JSON output keeps its existing envelopes and exit codes without presentation labels or decoration.

`tasc setup` offers the optional first-party Command Skill. It never installs it without your choice.

## Common tasks

- **Discover:** `tasc discover "review code changes"` searches local skills and expands to GitHub when available.
- **Install:** `tasc skill install review --source owner/repo --dry-run`, then repeat with `--yes` after reviewing the plan.
- **Create:** `tasc skill create --workspace ./skill-workspace --name review-helper` authors in an explicit workspace; preview and approve before writing.
- **Evaluate:** `tasc eval run skill:file:///path/to/skill --json` runs deterministic package checks without executing skill instructions.
- **Check:** `tasc consistency check <skill-id-1> <skill-id-2> --json` compares selected skills. `tasc security check <skill-id>` checks local content without executing it.

Lifecycle changes require approval. `--dry-run` previews changes; locally modified skills need an explicit `--force`. Evaluations and checks produce immutable local evidence, not a security or quality guarantee.

## Agent and Command Skill

`tasc agent` starts a Copilot SDK conversation and displays the Turbo banner in an interactive terminal. Install and authenticate GitHub Copilot CLI first. The bundled TASC Command Skill is optional and uses the same versioned CLI contracts; manage it with `tasc skill command install`, `update`, `repair`, and `uninstall`.

Agent-assisted documentation changes may propose pull requests; they do not publish site copy or releases directly.

## Documentation and releases

- [TASC 0.1.0 user guides](https://pascalvanderheiden.github.io/turbo-agent-skills-cli/v0.1.0/index.html)
- [Release downloads, checksums, and attestations](https://github.com/pascalvanderheiden/turbo-agent-skills-cli/releases)
- [Release governance and verification runbook](https://github.com/pascalvanderheiden/turbo-agent-skills-cli/blob/main/.github/RELEASE.md)

TASC does not self-update. Select one of the five published Linux x64/ARM64, macOS x64/ARM64, or Windows x64 targets, verify the SHA-256 checksum, and install a newer release explicitly.
<!-- pages-content:end -->

## Install and get started

The shell installer supports Linux x64/ARM64, macOS Intel/Apple Silicon, and Windows x64 from Bash. Windows users can use the native PowerShell installer above. It installs to `~/.local/bin` on Unix-like systems or `%LOCALAPPDATA%\Programs\TASC` on Windows; set `TASC_INSTALL_DIR` to choose another writable directory.

Install the verified v0.1.0 binary with the command above, then start with:

```sh
tasc setup
tasc skill list
tasc state status
```

GitHub discovery and lifecycle operations require GitHub CLI (`gh`) and authentication. Local inventory and analysis work offline. Waza is optional; TASC's package Evaluation does not run skill instructions or require a model.

## Command examples

```sh
tasc --banner
tasc discover "review code changes"
tasc skill search review --format json
tasc skill install review --source owner/repo --dry-run
tasc skill create --workspace ./skill-workspace --name review-helper --dry-run
tasc eval run skill:file:///path/to/skill --json
tasc consistency check <skill-id-1> <skill-id-2> --json
tasc security check --all-installed
tasc agent
```

Use `tasc --help` or `tasc <command> --help` for command options. Search and Discover do not install or edit skills; skill creation does not install or publish them. TASC keeps Local State and Evaluation, Consistency, Security, and Insight evidence local by default.

## TASC Command Skill

The first-party Command Skill is optional. `tasc setup` explains available adapters and scopes; choose explicitly before installing. Its release package is inspectable as `tasc-command-skill-v<version>.tar.gz`. The Skill checks the installed CLI's version and capabilities and never reimplements TASC policy.

## Releases and integrity

Stable releases contain five reproducible target archives, the Command Skill package, `SHA256SUMS.txt`, a CycloneDX SBOM, and GitHub build-provenance attestations. Release drafts require checks, maintainer review, and an approval environment before publication. Published releases and tags must be immutable.

Use the [versioned guides](https://pascalvanderheiden.github.io/turbo-agent-skills-cli/docs/) for installation, lifecycle safety, discovery, authoring, Evaluation, Consistency, Security, troubleshooting, and release verification. Repository owners must configure Pages, protected `v*` tags, the `release-approval` environment, and immutable releases as documented in the [release runbook](.github/RELEASE.md); workflows fail closed when required settings are absent.
