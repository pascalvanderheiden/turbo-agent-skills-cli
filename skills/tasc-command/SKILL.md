---
name: tasc-command
description: Use TASC's installed command-line interface to inspect, discover, evaluate, and safely manage Agent Skills.
metadata:
  tasc-source: tasc://first-party/command-skill
  tasc-path: tasc-command
  tasc-path: tasc-command
  tasc-version: 0.2.0
---

# TASC Command Skill

Use the installed `tasc` executable for every TASC operation. Do not edit TASC
Local State, installed skill files, or TASC evidence directly, and do not
reimplement TASC policy.

## Compatibility handshake

Before acting, run `tasc skill command capabilities --skill-version 0.2.0 --policy-semantics-version 1 --json`. Continue only when
the JSON envelope has `schemaVersion: 1`, the `skill command capabilities`
command, and the requested capability. Treat a missing executable as
`tasc_executable_missing` and return a structured diagnostic such as
`{"schemaVersion":1,"command":"skill command","diagnostic":{"code":"tasc_executable_missing","category":"dependency","origin":"tasc-command-skill","message":"The TASC executable is unavailable.","remediation":"Install TASC and put tasc on PATH."}}`.
If the handshake is unavailable, report degraded support and offer
read-only operations only when the installed command's JSON contract is known.
Never infer or emulate unknown security, profile, or approval semantics.

Use only capabilities reported by the handshake. If the skill and executable
versions differ, explain which operations are unavailable and prefer the
executable's advertised contract. A missing required policy capability blocks
the operation.

## Operating rules

- Use noun-first TASC commands and request versioned output with `--json` or
  `--format json`.
- Keep Profile, Adapter Target, and `user`/`project` scope explicit whenever an
  operation asks for them. Never substitute `--all`, `--all-supported`, or
  another broad selector unless the user explicitly chooses it.
- For mutations, first run the matching command with `--dry-run`, present its
  exact plan and security findings, then let TASC request approval. Never pass
  `--yes` on behalf of conversational consent. If the command cannot obtain
  approval interactively, show the exact command for the user to run in a
  terminal.
- Preserve structured diagnostics, including their `code`, `message`, and
  `remediation`; do not hide a failure behind a successful assistant response.
- GitHub search, preview, and remote lifecycle operations may require GitHub
  CLI authentication. Local inventory and analysis remain available offline.
  Optional Evaluation execution may require Waza; do not claim it ran when the
  dependency is unavailable.

## Stable operations

Use the corresponding operation only when its capability is advertised:

| User intent | TASC command |
| --- | --- |
| Inventory and Profile status | `tasc skill list --json`; `tasc state status --json` |
| Search or discover | `tasc skill search QUERY --json`; `tasc discover QUERY --json` |
| Install, update, or remove an Agent Skill | `tasc skill install`, `tasc skill update`, or `tasc skill remove` |
| Tracking or Watchlists | `tasc skill track`; `tasc watch list --json` |
| Author or validate a Skill | `tasc skill create`, `tasc skill validate`, or `tasc skill drafts` |
| Evaluation, Consistency, or Security | `tasc eval`, `tasc consistency`, or `tasc security` |
| Audit or statistics | `tasc audit --json`; `tasc stats --json` |
| Bundles | `tasc bundle validate MANIFEST --json` |
| Manage this Command Skill | `tasc skill command list|install|update|repair|uninstall` |

For any destructive or security-sensitive request, TASC's executable remains
the authority for scope resolution, policy decisions, plans, and approval.
