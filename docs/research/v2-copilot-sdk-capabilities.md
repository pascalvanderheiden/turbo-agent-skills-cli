# Research: Copilot SDK model, usage, and streaming capabilities

Research for #42, captured 2026-10-18 from first-party GitHub documentation
and source.

## Baseline

TASC pins `github-copilot-sdk = "=1.0.22"` with the `runtime` feature
(`crates/tasc-cli/Cargo.toml`, `Cargo.lock`). Crates.io maps that release to
Copilot SDK commit
[`ffd25c7`](https://github.com/github/copilot-sdk/tree/ffd25c775742f73e13a492093be0287d7885123f).
The latest source reviewed was `v1.0.23-preview.5` at
[`b96cd2b`](https://github.com/github/copilot-sdk/tree/b96cd2b54f7eb87458fd50f485a751c712e88f62).
The relevant documentation is materially the same; newer-source-only details
are called out below.

GitHub's maintained boundary between Copilot CLI and SDK features is
[`docs/troubleshooting/compatibility.md`](https://github.com/github/copilot-sdk/blob/ffd25c775742f73e13a492093be0287d7885123f/docs/troubleshooting/compatibility.md).

## Findings

### Models and configuration

- `Client::list_models()` calls `models.list` and returns model IDs, names,
  capabilities, policy/provider data, supported reasoning efforts, and
  optional billing data
  ([Rust client](https://github.com/github/copilot-sdk/blob/ffd25c775742f73e13a492093be0287d7885123f/rust/src/lib.rs#L3029-L3042),
  [generated type](https://github.com/github/copilot-sdk/blob/ffd25c775742f73e13a492093be0287d7885123f/rust/src/generated/api_types.rs#L17338-L17388)).
  It is the programmatic replacement for the CLI-only interactive `/model`
  picker.
- A model can be selected when creating/resuming a session or changed during
  one with `Session::set_model()`, which calls `session.model.switchTo`
  ([Rust session API](https://github.com/github/copilot-sdk/blob/ffd25c775742f73e13a492093be0287d7885123f/rust/src/session.rs#L1119-L1156)).
  `session.rpc().model().get_current()` reports committed/pending state
  ([session persistence](https://github.com/github/copilot-sdk/blob/ffd25c775742f73e13a492093be0287d7885123f/docs/features/session-persistence.md#changing-models-mid-session)).
- Auto routing tiers are separate from model selection. A tier switch may be
  accepted before it is active; observe `session.model_change` and
  `session.auto_tier_switch_failed`
  ([session persistence](https://github.com/github/copilot-sdk/blob/ffd25c775742f73e13a492093be0287d7885123f/docs/features/session-persistence.md#auto-tier-preference)).
- Model, tools, system message, working directory, MCP, custom-agent, and
  skill settings are session configuration. The SDK does not provide a
  TASC-level persisted default model. TASC must persist its own profile
  default and pass it on every create/resume
  ([compatibility matrix](https://github.com/github/copilot-sdk/blob/ffd25c775742f73e13a492093be0287d7885123f/docs/troubleshooting/compatibility.md),
  [resume options](https://github.com/github/copilot-sdk/blob/ffd25c775742f73e13a492093be0287d7885123f/docs/features/session-persistence.md#resume-options)).
- Constraint: the generated `Model` payload is explicitly an experimental
  wire-protocol surface. Pin versions and tolerate optional or changed fields.
  Newer source also documents server-level Auto-tier metadata behind
  `DYNAMIC_AUTO_TIERS`; do not rely on it with pinned `1.0.22`.
- **Product decision**: TASC's model choice defaults to the logical value
  `auto`. Treat it as TASC's sentinel for runtime automatic/default routing,
  not as a concrete model ID that must appear in `list_models()`. Do not
  persist the concrete model selected internally by automatic routing.
  Auto-tier preference remains a separate SDK setting.

### Usage and authentication

GitHub documents three complementary usage signals
([usage and billing](https://github.com/github/copilot-sdk/blob/ffd25c775742f73e13a492093be0287d7885123f/docs/features/usage-and-billing.md)):

| Need | Reliable API | Constraint |
|---|---|---|
| Per-model-call tokens | `assistant.usage` event | Ephemeral; includes sub-agent calls |
| Context-window fill | `session.usage_info` event or `session.metadata.contextInfo` | Metadata RPC is experimental |
| Accumulated session totals | `session.rpc().usage().get_metrics()` | Experimental and session-scoped |
| Per-model price metadata | `client.list_models()` → `Model.billing` | Model payload is experimental |

`assistant.usage` contains the model, input/output token counts, and billing
multiplier. `get_metrics()` returns accumulated model metrics,
`total_nano_aiu`, and premium-request cost. These are actual SDK/runtime
measurements; TASC should not estimate missing counts.

Usage is not durable account-wide accounting. Ephemeral events are not replayed
when resuming, and accumulated totals belong to the current session. TASC must
capture totals before teardown if it wants to retain a run summary. The SDK
does not own nano-AIU-to-credit conversion; billing semantics remain governed
by [GitHub Copilot usage documentation](https://docs.github.com/en/copilot/managing-copilot/understanding-and-managing-copilot-usage).

`Client::get_auth_status()` calls `auth.getStatus` and returns
`is_authenticated`, `auth_type`, `host`, `login`, and `status_message`
([Rust client](https://github.com/github/copilot-sdk/blob/ffd25c775742f73e13a492093be0287d7885123f/rust/src/lib.rs#L3021-L3026),
[response type](https://github.com/github/copilot-sdk/blob/ffd25c775742f73e13a492093be0287d7885123f/rust/src/types.rs#L6137-L6155)).
Treat `auth_type` as an extensible string. Authentication precedence is
documented in the SDK's
[authentication guide](https://github.com/github/copilot-sdk/blob/ffd25c775742f73e13a492093be0287d7885123f/docs/auth/README.md).

### Streaming, cancellation, progress, and tools

With `SessionConfig::with_streaming(true)`, the SDK emits ephemeral deltas
alongside persisted final events
([streaming events](https://github.com/github/copilot-sdk/blob/ffd25c775742f73e13a492093be0287d7885123f/docs/features/streaming-events.md)).
TASC already enables streaming.

- Response lifecycle: `assistant.turn_start`,
  `assistant.message_delta`, `assistant.message`, `assistant.turn_end`, and
  `assistant.usage`.
- Tool lifecycle: `tool.execution_start`, optional `tool.shell_output` and
  `tool.execution_progress`, then `tool.execution_complete`.
- Visibility: tool events expose tool-call ID/name, parsed arguments, MCP
  identity, live secret-filtered shell output, human-readable progress,
  success/error/result, model/interaction ID, and whether the user requested
  the call. Envelope `agentId` attributes sub-agent events.
- Progress: there is no authoritative overall percentage. A UI can reliably
  show turn state, active tools, tool progress/output, and context-window use.
- Persistence: deltas, progress, shell output, and usage events are ephemeral
  and are not replayed after resume.

`Session::abort()` sends `session.abort` and is cancel-safe
([Rust session API](https://github.com/github/copilot-sdk/blob/ffd25c775742f73e13a492093be0287d7885123f/rust/src/session.rs#L1109-L1117)).
Cancellation is observable through an `abort` event and
`session.idle { aborted: true }`. Replacing tools does not cancel an in-flight
turn; replace tools while idle or abort first
([changing tools](https://github.com/github/copilot-sdk/blob/ffd25c775742f73e13a492093be0287d7885123f/docs/features/changing-tools.md)).

### Compatibility and errors

- `client.get_status()` reports the CLI package and negotiated JSON-RPC
  protocol versions
  ([Rust client](https://github.com/github/copilot-sdk/blob/ffd25c775742f73e13a492093be0287d7885123f/rust/src/lib.rs#L3015-L3018)).
  Pinned `1.0.22` supports protocol v2-v3 adapters
  ([compatibility](https://github.com/github/copilot-sdk/blob/ffd25c775742f73e13a492093be0287d7885123f/docs/troubleshooting/compatibility.md#protocol-versioning)).
- The Rust error model separates protocol/version, RPC code, session,
  I/O/JSON, missing binary, invalid configuration, and token-provider errors
  ([`errors.rs`](https://github.com/github/copilot-sdk/blob/ffd25c775742f73e13a492093be0287d7885123f/rust/src/errors.rs#L120-L252)).
  TASC should match typed error kinds rather than error-message text.
- Experimental surfaces relevant here include `models.list` payloads,
  `session.usage.getMetrics`, context metadata/recomputation, and quota RPCs.
  Depend on them only with pinned SDK/runtime versions and graceful
  unsupported/shape-change handling.

## TASC decisions

1. Use `get_auth_status()` before model/session operations; do not infer auth
   state from generic failures.
2. Default every agent to `auto`. Populate alternative fixed-model choices
   from `list_models()` and switch them with `set_model()`. `/model` follows
   the v2 map's existing semantics: a profile selection persists in TASC
   Local State, while a session override applies only to the active session.
   Selecting `auto` clears any fixed-model choice and returns to runtime
   automatic/default routing; it must not depend on an `auto` entry being
   returned by pinned `1.0.22`'s experimental model-list payload.
3. Report only SDK-supplied usage: live `assistant.usage` plus
   `usage.get_metrics()` before teardown. Do not synthesize token counts or
   hard-code billing conversion.
4. Extend the existing event subscription for tool lifecycle, progress,
   shell output, usage, and abort state. Do not promise a percentage complete.
5. Use typed error variants, report runtime/protocol versions from
   `get_status()`, and degrade cleanly when experimental RPCs are unsupported.
6. Re-check the compatibility matrix and generated experimental markers on
   every SDK upgrade.

## Remaining uncertainty

- GitHub billing documentation, not the SDK, owns nano-AIU/credit and
  premium-request conversion rules.
- `models.list`, usage metrics, context metadata, and quota surfaces may
  change without a stable major-version contract.
- Account/quota RPCs were documented but not needed to answer actual
  per-session token usage; verify their Rust call sites before building quota
  UI.
