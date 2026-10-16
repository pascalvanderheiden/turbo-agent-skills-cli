# Research: Copilot SDK and `gh` integration constraints

Research for #3, captured 2026-10-16. This document treats both GitHub
Copilot SDK and GitHub CLI as required foundations, then identifies what TASC
should delegate, wrap, or implement in its shared Rust core.

## Executive decision

TASC should be a typed Rust orchestration layer around two required external
capabilities:

1. **Delegate GitHub-hosted skill lifecycle operations to `gh skill`.**
   Installation, discovery, listing, preview, update, validation, and
   publication already exist in GitHub CLI and should not be rebuilt.
2. **Wrap GitHub Copilot SDK as the required agent runtime.** The SDK owns
   sessions, model interaction, tool execution, custom agents, skills, hooks,
   and runtime events. TASC should not implement an agent harness.
3. **Implement only TASC-specific product logic in the shared core.** That
   includes canonical local state, watchlists, cross-skill Consistency Checks,
   Evaluation definitions/results, policy, and typed orchestration across
   `gh` and the SDK.

Both integrations cross process or RPC boundaries. The shared core therefore
needs first-class async command execution, cancellation, timeouts, structured
errors, version/capability detection, and test doubles.

## Source baseline

Primary sources used:

- GitHub Copilot SDK repository at commit
  [`b04171f`](https://github.com/github/copilot-sdk/tree/b04171f698fd80bfd5988505fd93d4a9b80a3934),
  including its [root README](https://github.com/github/copilot-sdk/blob/b04171f698fd80bfd5988505fd93d4a9b80a3934/README.md),
  [Rust README](https://github.com/github/copilot-sdk/blob/b04171f698fd80bfd5988505fd93d4a9b80a3934/rust/README.md),
  [Rust Cargo features](https://github.com/github/copilot-sdk/blob/b04171f698fd80bfd5988505fd93d4a9b80a3934/rust/Cargo.toml),
  [authentication docs](https://github.com/github/copilot-sdk/blob/b04171f698fd80bfd5988505fd93d4a9b80a3934/docs/auth/README.md),
  and [skills docs](https://github.com/github/copilot-sdk/blob/b04171f698fd80bfd5988505fd93d4a9b80a3934/docs/features/skills.md).
- GitHub CLI source at commit
  [`b758ce3`](https://github.com/cli/cli/tree/b758ce35643e64cfe4d61bc58b6009c4088e5e0c),
  especially the built-in
  [`pkg/cmd/skills`](https://github.com/cli/cli/tree/b758ce35643e64cfe4d61bc58b6009c4088e5e0c/pkg/cmd/skills)
  command and
  [extension dispatcher](https://github.com/cli/cli/blob/b758ce35643e64cfe4d61bc58b6009c4088e5e0c/pkg/cmd/extension/manager.go).
- Installed GitHub CLI `2.101.0` command help for `gh skill` and all its
  subcommands. GitHub CLI itself identifies the command group as preview and
  subject to change without notice.
- GitHub's official
  [extension authoring documentation](https://docs.github.com/en/github-cli/github-cli/creating-github-cli-extensions).
- Agent Skills
  [format specification](https://agentskills.io/specification), at source
  commit
  [`69ef37e`](https://github.com/agentskills/agentskills/tree/69ef37e9424c0a7ea9dd2293b559e43ec8176379).

## GitHub Copilot SDK: required agent harness

### Capabilities TASC gets from the SDK

The SDK exposes the same agent runtime used by Copilot CLI. GitHub explicitly
positions it as the layer that handles planning, tool invocation, file edits,
and orchestration, with first-party packages for Rust and five other
languages. It is generally available and follows semantic versioning
([SDK README](https://github.com/github/copilot-sdk/blob/b04171f698fd80bfd5988505fd93d4a9b80a3934/README.md)).

The Rust API provides:

- session creation/resume, streaming events, model selection, tools,
  permission and elicitation handlers, custom agents, hooks, MCP, and typed
  low-level RPC access;
- filesystem-backed skills through `skillDirectories` and `disabledSkills`;
- an experimental `SkillProvider` for application-owned catalogs;
- experimental, capability-negotiated skill installation review APIs.

The filesystem skill contract matches TASC's domain: a named directory with a
`SKILL.md`. `skillDirectories` points at a parent directory and the runtime
discovers immediate skill subdirectories
([skills docs](https://github.com/github/copilot-sdk/blob/b04171f698fd80bfd5988505fd93d4a9b80a3934/docs/features/skills.md)).
That lets the agent consume the same project-level `.agents/skills` directory
that `gh skill install` populates.

### Architecture and Rust constraints

The normal architecture is:

```text
TASC agent surface
  -> github-copilot-sdk Rust client
  -> JSON-RPC 2.0 over stdio or TCP
  -> copilot --server
```

The SDK manages process start, health checks, and shutdown. Rust also supports
externally supplied streams and in-process runtime options, but those do not
change the product boundary: TASC talks to the Copilot runtime through the
SDK contract, not by implementing agent behavior itself
([Rust architecture](https://github.com/github/copilot-sdk/blob/b04171f698fd80bfd5988505fd93d4a9b80a3934/rust/README.md#architecture)).

Current Rust constraints:

- The crate requires Rust 1.94 or later
  ([Rust prerequisites](https://github.com/github/copilot-sdk/blob/b04171f698fd80bfd5988505fd93d4a9b80a3934/rust/README.md#prerequisites)).
- Its current Cargo default enables `bundled-cli`; disabling default features
  yields an external-stream-only client, while the `runtime` feature manages
  a supplied runtime without embedding the bundle
  ([Cargo features](https://github.com/github/copilot-sdk/blob/b04171f698fd80bfd5988505fd93d4a9b80a3934/rust/Cargo.toml#L26-L34)).
- Managed runtime resolution is explicit path, `COPILOT_CLI_PATH`, then the
  bundled runtime; the Rust docs state there is no PATH scan
  ([Rust `ClientOptions`](https://github.com/github/copilot-sdk/blob/b04171f698fd80bfd5988505fd93d4a9b80a3934/rust/README.md#client)).
- Authentication priority is explicit SDK token, direct Copilot API
  environment auth, environment GitHub tokens, stored Copilot CLI
  credentials, then GitHub CLI credentials
  ([authentication priority](https://github.com/github/copilot-sdk/blob/b04171f698fd80bfd5988505fd93d4a9b80a3934/docs/auth/README.md#authentication-priority)).

**Implication:** Copilot SDK is a required product foundation, but the SDK
crate should sit behind an `AgentRuntime` adapter used by the agent-facing
binary/crate. The shared domain crate and lifecycle-only CLI paths should not
need to embed or start the Copilot runtime. This is dependency isolation, not
an optional agent capability.

Experimental SDK surfaces must be capability-detected. TASC must not make
`SkillProvider` or installation-confirmation RPCs its only path: stable
filesystem skills plus `gh skill install` remain the fallback.

## `gh skill`: required lifecycle foundation

`gh skill` is a built-in GitHub CLI command, not an extension. GitHub CLI
source contains native `install`, `list`, `preview`, `publish`, `search`, and
`update` implementations
([source](https://github.com/cli/cli/tree/b758ce35643e64cfe4d61bc58b6009c4088e5e0c/pkg/cmd/skills)).
The installed `gh 2.101.0` help labels the entire command group preview.

The current command surface provides:

- **Install:** remote GitHub repository or local directory; project/user
  scope; many agent hosts; version resolution and pinning; injected source
  metadata; shared `.agents/skills` placement for GitHub Copilot and several
  other hosts.
- **List:** installed skills across known hosts and scopes, with structured
  JSON fields: `agentHosts`, `path`, `pinned`, `scope`, `skillName`,
  `sourceURL`, and `version`.
- **Preview:** inspect a skill and adjacent files before installation.
- **Search:** GitHub-wide skill discovery through GitHub Code Search.
- **Update:** compare recorded local tree SHA with the source repository,
  support dry-run, pinning, and forced restoration.
- **Publish:** validate skills against the Agent Skills specification and
  publish a GitHub release; `--dry-run` validates without publication.

**Implication:** TASC should invoke `gh` as a subprocess and parse structured
output wherever available. There is no official Rust binding to GitHub CLI's
Go internals, and directly reproducing its API calls, credential handling,
host-path rules, source metadata, or update algorithm would violate the
required "reuse `gh`" boundary.

The core should expose a narrow typed `GhSkillGateway`, not generic shell
strings. Commands should use explicit non-interactive flags, machine-readable
output, sanitized environment inheritance, bounded execution, captured
stdout/stderr, and typed exit/error mapping. Preview or other human-rendered
commands can be surfaced as delegated interactive actions rather than parsed.

Because `gh skill` is preview, startup or first use should probe `gh version`,
`gh skill --help`, and required flags/JSON fields. Unsupported versions need
an actionable error; TASC should not silently fall back to reimplementation.

## GitHub CLI extensions: distribution option, not core integration

GitHub CLI extensions are repositories named `gh-<name>` and may be
interpreted scripts or precompiled binaries in any language
([official docs](https://docs.github.com/en/github-cli/github-cli/creating-github-cli-extensions)).
The CLI dispatches an extension as a separate process, forwarding argv and
stdio and setting `GH_EXTENSION=1` and `GH_PATH`
([dispatcher source](https://github.com/cli/cli/blob/b758ce35643e64cfe4d61bc58b6009c4088e5e0c/pkg/cmd/extension/manager.go#L94-L143)).

TASC can therefore be distributed later as a precompiled Rust `gh-tasc`
extension, but that does not grant in-process access to `gh`. It is only an
alternate command/distribution shell around the same TASC binary and shared
core. Initial GitHub Releases distribution does not need this.

## Capability matrix

| Capability | Required foundation | TASC action | Shared-core implication |
|---|---|---|---|
| Install from GitHub/local source | `gh skill install` | **Delegate** | Typed subprocess request/result; persist TASC references, not a second installer |
| List installed skills | `gh skill list --json ...` | **Delegate** | Normalize JSON into TASC domain types |
| Preview before install | `gh skill preview` | **Delegate** | Launch delegated command; do not recreate its browser/pager UX |
| Search public skills | `gh skill search` | **Delegate** | Wrap search query/results; no separate GitHub Code Search client |
| Detect/apply source updates | `gh skill update` | **Delegate** | Watchlist schedules and policy are TASC-owned; source comparison/install is `gh`-owned |
| Validate/publish a skill repository | `gh skill publish` | **Delegate** | Use `--dry-run` for conformance checks; do not duplicate publication or authoritative validation |
| Parse skill metadata for TASC analysis | Agent Skills specification | **Implement minimally** | Read only fields needed for inventory, Evaluation, and Consistency Checks; do not claim authoritative publish validation |
| Load installed skills into a Copilot session | Copilot SDK `skillDirectories` | **Wrap** | Point sessions at paths returned/managed through `gh skill` |
| Run and observe agent turns | Copilot SDK | **Wrap** | SDK adapter owns sessions/events/tools; shared core owns use-case orchestration |
| In-memory/application skill catalog | Copilot SDK `SkillProvider` | **Wrap cautiously** | Experimental capability; filesystem fallback required |
| Agent-confirmed install workflow | SDK experimental install APIs + `gh skill` | **Wrap cautiously** | Capability-detect SDK review; use `gh skill install` as stable lifecycle executor/fallback |
| GitHub/Copilot authentication | `gh auth` and SDK credential chain | **Delegate** | No TASC credential store; never parse `hosts.yml` directly |
| Canonical local inventory and watchlists | Neither | **Implement** | TASC-owned local model linking sources, installs, policies, and observations |
| Cross-skill Consistency Checks | Neither | **Implement** | Pure shared-core rules and findings, reusable by CLI and agent |
| Evaluation definitions/results | Neither; SDK is execution engine | **Implement** | Shared-core scenarios, assertions, traces, and scoring; SDK adapter runs Copilot turns |
| Distribute as `gh tasc` | `gh extension` | **Optional wrapper** | Same executable/core; no separate business logic |

## Required shared-core seams

The architecture should make these boundaries explicit:

```rust
trait GhSkillGateway {
    // install, list, search, update, validate/publish delegation
}

trait AgentRuntime {
    // SDK-backed session creation, execution, events, and cancellation
}

trait SkillRepository {
    // TASC-owned inventory, watchlists, evaluations, and findings
}
```

Exact Rust signatures belong in implementation design, but the ownership rule
is fixed:

- adapters translate external process/RPC contracts;
- the shared core owns domain state and use cases;
- CLI and SDK agent surfaces call the same use cases;
- neither surface contains independent lifecycle, Evaluation, or Consistency
  Check business logic.

## Risks and implementation guardrails

1. **Preview drift:** `gh skill` flags and JSON fields may change. Pin a tested
   minimum `gh` version, probe capabilities, and keep contract fixtures.
2. **Experimental SDK drift:** capability-negotiate `SkillProvider` and install
   review APIs; keep filesystem and `gh skill` fallback paths.
3. **Subprocess failures:** distinguish missing executable, unsupported
   version, authentication failure, cancellation, timeout, malformed JSON,
   and command failure. Never return success-shaped empty results.
4. **Runtime packaging:** decide deliberately between bundled SDK runtime,
   supplied runtime, and external streams. Keep this choice outside the
   shared domain crate.
5. **Local-first trust boundary:** pass only explicitly selected skill paths
   and Evaluation inputs to the SDK. Do not add hosted synchronization as a
   side effect of either adapter.

## Conclusion

The foundations cover more than transport: `gh skill` owns the GitHub skill
lifecycle, while Copilot SDK owns agent execution. TASC's durable value is the
typed local model and orchestration between them, plus the uncovered
Evaluation and Consistency Check workflows. Reimplementing either foundation
would increase compatibility, authentication, update, and maintenance risk
without adding product value.
