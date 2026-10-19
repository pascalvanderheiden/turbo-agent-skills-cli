# Research: Terminal UX baseline from existing agent/CLI references

Research for [#94](https://github.com/pascalvanderheiden/turbo-agent-skills-cli/issues/94) (part of map [#93](https://github.com/pascalvanderheiden/turbo-agent-skills-cli/issues/93)), captured 2026-10-19.

## Question

What existing patterns, constraints, and visual references in this repo — and in official GitHub Copilot CLI documentation/source — should anchor the final terminal design for TASC's CLI and dedicated agent, so the UI feels coherent, terminal-safe, and familiar?

## Scope and method

Primary sources only, as instructed by #94: this repository's current Rust CLI/agent/banner/docs/prototypes, and the design direction captured live in #93/#94; for GitHub Copilot CLI behavior, only `docs.github.com` pages and the GitHub-owned `github/copilot-cli` repo (release/download surface only — the CLI's TUI implementation itself is not open source, so claims below are drawn strictly from what GitHub's own docs assert about its behavior). No implementation changes were made; this is read-only research.

---

## 1. What #93/#94 already decided (binding destination, not yet implemented)

Quoted directly from the live issue bodies (`gh issue view 93`, `gh issue view 94`, fetched 2026-10-19T07:08Z/07:05Z):

- Input stays at the bottom; output occupies the scrollable middle; a compact status line sits **below the input**, with active model and token usage at the right when available.
- Color and icons must separate user input, agent output, state, warnings, and actions, while remaining **terminal-safe and accessible**.
- Slash Commands use **type-to-filter discovery and command-specific pickers**; `/model` opens a model picker; the same interaction grammar covers every Slash Command.
- Skill inventory browsing needs **pagination or virtual scrolling**, clear focus, and efficient selection.
- The banner is **static**, appears at the top, and naturally scrolls away after interaction; the **animated banner must be removed**.
- The README/GitHub Pages direction should use a **real agent terminal screenshot** based on the current agent screenshot and finalized terminal design, rather than animated artwork.
- Explicitly out of scope for this research ticket: implementing the redesigned surfaces, replacing scriptable/machine-readable output with a full-screen-only interface, GUI-only/browser-only requirements, and unrelated feature expansion.

`CONTEXT.md` (repo root) fixes the canonical vocabulary this design must keep using: **Slash Command**, **Agent Session**, **Inventory Projection**, **Documentation Surface**, **Public Delivery** — e.g. "A deterministic command addressed to the TASC agent using noun-first syntax, with compatibility aliases normalized to one canonical command. Slash Commands and natural-language requests share the same authorized TASC operations" (`CONTEXT.md`, Slash Command entry).

**Gap:** the "current agent screenshot" referenced by #93 is not attached to #93/#94 and no terminal screenshot image exists in the repository (`find` for `*.png|*.jpg|*.jpeg|*.gif` under `docs/` returned nothing). The only existing brand asset is `docs/site/assets/tasc-speed-banner.svg`, an ASCII-art SVG, referenced from `README.md:3` (`![TASC speed ASCII art banner](docs/site/assets/tasc-speed-banner.svg)`). A follow-on ticket should attach the intended reference or confirm that the documentation image will be captured fresh from the finalized prototype.

## 2. Current repo CLI/agent implementation (the actual starting point)

### 2.1 Banner — animated today, contradicts the "static" decision

`crates/tasc-cli/src/banner.rs:11-73` implements a 32-frame (`FRAME_COUNT`), 75 ms-per-frame (`FRAME_DELAY`) ANSI redraw animation: it hides the cursor (`CursorGuard::hide`, `banner.rs:19-24`), clears and repaints the screen each frame with `\x1b[H\x1b[J` (`banner.rs:66`), and moves a `━━━━⚡` streak across 16 cycles before settling on the static ASCII logo (`LOGO_LINES`, `banner.rs:6-12`). It already degrades safely for non-terminal output: `if !stdout.is_terminal() { … return a single static frame }` (`banner.rs:59-62`), and respects `NO_COLOR` (`banner.rs:34-36`). `README.md:36` documents this directly: "`tasc --banner` plays the opt-in animated Turbo banner. It uses ANSI cursor redraws only in an interactive terminal and prints a safe static frame when piped." `README.md:46` also states the dedicated agent "displays the Turbo banner in an interactive terminal" on `tasc agent` startup.

A second, near-duplicate banner exists in the agent path: `crates/tasc-cli/src/agent.rs:965-986` defines its own `TURBO_BANNER_LINES`/`RAINBOW_ANSI` constants and a `turbo_banner(color: bool)` function that prints the same 5-line ASCII logo colorized per-line with a fixed 5-color ANSI rotation (`"91","93","92","96","94"` = bright red/yellow/green/cyan/blue, `agent.rs:971`). This banner is **not animated** (no frame loop, no sleep) — it's a single static, optionally rainbow-colored render, invoked once per `/status`/dashboard print (`agent.rs:1020-1023`, `render_dashboard`).

**Implication for #93's "remove the animated banner, make it static, top, and scroll away":** the agent-session banner (`agent.rs`) already matches that destination structurally (static, prints once, part of normal scrollback). The CLI's `tasc --banner`/`banner.rs` animation is the one that must be retired or converted to render its single final frame only, per the explicit #93 instruction; the two should likely be unified onto one static banner implementation (two divergent `LOGO_LINES`/`TURBO_BANNER_LINES` constants and two color policies is itself a latent inconsistency worth flagging to implementers, not resolving here).

### 2.2 Color policy — `NO_COLOR` + TTY-gated, already the terminal-safe baseline to keep

`agent.rs:1013-1019`:
```rust
fn output_policy(interactive: bool, no_color: bool, json_output: bool) -> (bool, bool) {
    (
        interactive || json_output,
        interactive && !no_color && !json_output,
    )
}
```
called as `output_policy(interactive, std::env::var_os("NO_COLOR").is_some(), context.json_output)` (`agent.rs:1063-1067`). This is the existing, tested terminal-safe gate: color is enabled only when stdin+stdout are both terminals (`std::io::stdin().is_terminal() && std::io::stdout().is_terminal()`, `agent.rs:1062`), `NO_COLOR` is unset, and output is not JSON. The same `NO_COLOR` check and `IsTerminal` gate exists independently in `banner.rs:34-36,57-58`. Tests pin this: `agent.rs:1819-1827` assert the JSON dashboard path is colorless, the plain dashboard omits ANSI, and the "rainbow" dashboard contains every `RAINBOW_ANSI` code. Any new picker/status-line/icon work should reuse this existing `is_terminal() && !NO_COLOR && !json` triple-gate rather than inventing a second one — it is the one piece of "terminal-safe color" infrastructure that already exists and is unit-tested.

Color usage today is minimal and decorative only (rainbow banner letters); there is **no established color-coding convention yet for separating user input / agent output / state / warnings / actions** as #93 requires — `print_shell_event` (`agent.rs:1322-1337`) only differentiates by routing certain `kind`s (`"error"`, `"warning"`, `"activity"`, `"plan"`) to stderr vs stdout and by a plain `"{kind}: {message}"` text prefix, with no color applied at all. This is the concrete gap the palette/icon decision in #93 needs to fill; there is no pre-existing in-repo precedent to preserve beyond the triple-gate above.

### 2.3 Icons — none exist in the terminal surfaces today

Grepping `crates/tasc-cli/src` and `crates/tasc-agent/src` found exactly one Unicode glyph used anywhere in rendered output: the `⚡` lightning bolt in the animated banner's motion streak (`banner.rs:46`, asserted present exactly once by `banner.rs:85-91`'s test). No status glyphs, checkmarks, spinners, or semantic icons exist elsewhere in `agent.rs`'s event/dashboard rendering (`shell_event_output`, `dashboard_output`, `print_help`) — these are plain-text, prefix-labeled lines (e.g. `"result: …"`, `"error: …"`, `"GitHub CLI: authenticated · Model: … · Tokens: … · Skills: …"`, `agent.rs:991-1011`). Any icon semantics introduced for #93 are new vocabulary, not a migration of an existing scheme; they should be chosen with a text-only / `NO_COLOR` fallback given the CLI's existing non-TTY branch already strips color and must continue to render readably without Unicode icon support assumed.

### 2.4 Slash commands today — flat parser, no type-to-filter, no picker

`parse_shell_command` (`agent.rs:737-808`) is a single synchronous function matching a trimmed line against a fixed command table (`/quit`, `/help`, `/status`, `/approve`, `/deny`, `/cancel`, `/model …`, `/audit`, `/stats`, `/profile status`, `/profiles`, `/bundle validate`, `/command-skill …`, and a generic `/skill|/evaluation|/eval|/consistency|/security|/profile|/profiles|/bundle|/bundles|/command-skill|/command-skills` fallback that re-wraps arguments as a natural-language tool request). Input is read line-by-line from stdin via `tokio::io::BufReader`/`read_line` inside a plain `loop` with a `"tasc> "` prompt (`agent.rs:1053-1322`); there is **no readline-style editing, no live filtering as the user types, and no picker UI** — commands are only validated/rejected *after* Enter is pressed, returning a plain `Err(String)` usage message (e.g. `"Usage: /model [list|MODEL|profile MODEL]"`, `agent.rs:762`). `/help` enumerates the full command list as one joined string (`print_help`, `agent.rs:1339-1364`) rather than an interactive, filterable list. This is the single largest gap against #93's "type-to-filter discovery and command-specific pickers" requirement — the current interaction model is a classic blocking REPL, not a picker-driven TUI.

### 2.5 Model selection today — argument-based, not a picker; already has session vs. profile split worth preserving

`/model` parses to one of `ShellCommand::ModelList | ModelSession(String) | ModelProfile(String)` (`agent.rs:761-765`). `ModelList` calls `client.list_models().await` via the Copilot SDK (`list_models`, `agent.rs:1024-1029`) and prints `"Models: auto (default), {sorted ids}"` plus the current session model (`agent.rs:1139-1151`) — i.e. the full model list is already available from the SDK session and could back a real picker, but today it is only ever printed as flat text, never offered as a navigable list. `ModelSession`/`ModelProfile` both validate against that same list before calling `session.set_model` (`validate_model_choice`, `agent.rs:1042-1051`; `set_session_model`, `agent.rs:1031-1040`), and `ModelProfile` additionally persists the chosen model to a profile-scoped JSON file via atomic temp-file-then-rename (`save_profile_model`, `agent.rs:839-868`; `model_preferences_path`, `agent.rs:814-816`), while `ModelSession` only changes the running session, leaving the saved profile preference untouched — tested explicitly in `profile_model_preference_is_separate_from_session_default` (`agent.rs:1796-1809`). **This session-vs-profile distinction is a real, tested product decision #93's model picker must preserve** (the picker needs two distinguishable actions — "use for this session" vs. "save as my default" — not just a single selection), even though the *presentation* must change from argument parsing to a picker.

The status/dashboard line already carries the active model and token usage, matching #93's requirement almost exactly: `dashboard_output` renders `"GitHub CLI: {auth} · Model: {model} · Tokens: {total} · Skills: {n} ({tracked} tracked, {watched} watched)\n{onboarding}"` (`agent.rs:988-1011`), and `/status` re-renders the same line on demand without color (`agent.rs:1130-1152`, passing `color=false` explicitly). The remaining gap is purely layout: today the banner+dashboard print **above** the prompt once at session start (and again on `/status`), not as a persistently-redrawn status line **below** the input as #93 specifies.

### 2.6 Pagination/scrolling — not implemented anywhere

No pagination, virtual scrolling, or paging primitive exists in `tasc-cli` or `tasc-agent`: `render_inventory` (`crates/tasc-cli/src/main.rs:3355`) and other list renderers print every item in a single pass with `println!`, relying on the terminal's native scrollback. #93's "pagination or virtual scrolling… for Skill inventory browsing" is accordingly a net-new capability, not a refinement of existing logic.

### 2.7 The HTML "interaction prototype" is a GUI reference only, not a terminal layout precedent

`docs/prototypes/tasc-agent-interaction.html` (declared a "Throwaway logic prototype · issue 12" in its own markup, line 44) is a two-column desktop web page: a left `.panel` showing `#state` rows and action buttons, and a right `.panel` with a `#log` scrolling div (`log` class, line 70) capped at `min-height:270px`. It has **no bottom-fixed input bar, no top banner, and no status line below an input** — it is useful only for the *lifecycle/approval state machine* it encodes (reset/install/consistency-check flows), not for terminal layout. #93 itself instructs "finalized prototype" imagery for the screenshot, implying a new terminal-shaped prototype is expected to supersede this HTML one for the documentation screenshot; this file should not be mistaken for an existing terminal-layout reference.

## 3. Official GitHub Copilot CLI patterns (from docs.github.com only)

The GitHub Copilot CLI (`github/copilot-cli` on GitHub) is closed-source binary distributed via npm/Homebrew/WinGet/install script (`docs.github.com/en/copilot/how-tos/set-up/install-copilot-cli` → redirects to `.../copilot-cli/set-up-copilot-cli/install-copilot-cli`); no claim below relies on reading its source, only on GitHub's own documentation of its behavior.

### 3.1 Layout and status line

- The interactive interface is described as a conversation plus a dockable **sessions sidebar**, not a single-pane transcript: "The sessions sidebar is a panel docked beside your current conversation" (`docs.github.com/en/copilot/reference/copilot-cli-reference/cli-command-reference`, "The sessions sidebar" section).
- A **footer** is an explicit, user-configurable part of the layout, separate from the transcript: settings such as `footer.showQuota` and `footer.showBranch` are documented as toggling content "displayed in the footer" (`.../customize-copilot/change-settings`, "Settings you might want to change" and the `copilot config footer.showQuota off` example in `.../cli-command-reference`, "Using `copilot config`" section). This directly corroborates #93's "compact status line below the input" as an established CLI-agent convention, not a TASC invention.
- Welcome/empty state is documented with an official reference screenshot: `![Screenshot of the Welcome message in the interactive interface of Copilot.](/assets/images/help/copilot/copilot-cli-welcome.png)` (`docs.github.com/en/copilot/concepts/copilot-surfaces/copilot-cli`, "Interactive interface" section) — i.e. GitHub's own docs use a *real terminal screenshot*, not animated art, for the canonical "what this looks like" image. This is the direct precedent for #93's "use a real agent terminal screenshot… rather than animated artwork."

### 3.2 Color and accessibility — color is explicitly documented as unreliable alone

Describing the sessions-sidebar status dots, GitHub's own docs state the governing accessibility rule directly:

> "The color names above are a guide. The exact shades depend on your terminal's color scheme and the theme you have set with `/theme`. For example, the `github` theme uses its own fixed colors, and the `colorblind` theme uses blue and yellow in place of green and red. On terminals without full color support, the dots use your terminal's own green, blue, and yellow… the steady or pulsing shape and the `!`, `?`, and `○` markers are the most reliable cues." (`docs.github.com/en/copilot/reference/copilot-cli-reference/cli-command-reference`, "Session status indicators" section)

This is the single most load-bearing citation for #93's "terminal-safe and accessible" color requirement: **shape/glyph/motion must carry the meaning; color is a reinforcing layer, not the sole signal**, because terminal theme, 16-color fallback, and colorblind themes can all change or collapse the palette. Copilot CLI ships five built-in themes selectable via `/theme`/`theme` setting: `default`, `github`, `dim`, `high-contrast`, `colorblind` (`.../customize-copilot/change-settings`, "Settings you might want to change": `theme — Color palette (default, github, dim, high-contrast, or colorblind)`). A `colorblind` theme being first-class in GitHub's own CLI is itself a concrete design constraint TASC's palette decision should match or at least not contradict.

### 3.3 Slash-command and picker grammar

- `/settings` is documented as the canonical **type-to-filter, single-grammar picker** GitHub's own CLI uses for configuration: "Run `/settings` on its own to open a searchable editor that lists every setting… As you move up or down through the list, a short description of the highlighted setting is shown at the bottom of the editor" (`.../customize-copilot/change-settings`, "Opening the settings editor"), with documented keys `↑/↓` (move), `/` (search and filter the list), `Enter` (toggle/edit), `Ctrl+R` (reset to default), `Ctrl+E` (open `settings.json` in `$EDITOR`), `Esc` (clear search or close) (same page, keyboard-shortcut table). This is a concrete, citable reference implementation of "type-to-filter discovery and command-specific picker" that #93 asks TASC to emulate (not copy verbatim, per #93's explicit "without copying unsupported behavior").
- Multiple other pickers follow the **same grammar family** rather than bespoke UIs per command: `/agent` lets a user "select from the list of available custom agents" (`.../use-copilot-cli/overview`, "Use custom agents"); `/mcp add` is a form navigated with `Tab` between fields and committed with `Ctrl+S` (same page, "Add an MCP server"); file references autocomplete as you type `@` + a partial path, "the matching paths are displayed below the prompt box. Use the arrow keys to select a path and press `Tab` to complete the path" (same page, "Include a specific file in your prompt"). Each uses the same core primitives: type-to-filter, arrow-key navigation, a committing key, an escape/cancel key — exactly the "same interaction grammar should cover every Slash Command" principle #93 states.
- The full catalogue of global, in-session trigger characters is documented in one table: `@ FILENAME` (file context), `# NUMBER` (issue/PR context), `! COMMAND` (direct shell bypass; bare `!` enters a shell-mode REPL), a lone `$` + Enter (hands the terminal to a real shell), `?` on an empty prompt (quick help toggle), `Esc` (cancel; double-press interrupts a running turn), `Ctrl+C` (cancel/clear; twice exits), `Ctrl+D` (shutdown), `Ctrl+G` (edit prompt in `$EDITOR`), `Ctrl+L` (clear screen), `Ctrl+Enter`/`Ctrl+Q` (queue a message while the agent is busy), `Ctrl+R` (reverse-search command history) (`.../cli-command-reference`, "Global shortcuts in the interactive interface"). This is the authoritative source for what "terminal-native, not GUI-only" keybinding density looks like in a polished Copilot-adjacent CLI.

### 3.4 Model selection

- GitHub's docs describe model choice as a first-class, session-scoped setting with its own breakdown view, not folded only into a status line: `/usage` shows "a breakdown of token usage per model" and `/context` gives "a visual overview of your current token usage" (`.../use-copilot-cli/overview`, "Context management"). `copilot --agent=AGENT --prompt "…"` and analogous programmatic flags show model/agent selection is mirrored between the interactive picker and a scriptable flag (`.../use-copilot-cli/overview`, "Use custom agents"; `.../cli-programmatic-reference`, `--agent=AGENT` row) — directly analogous to TASC's existing `/model` (interactive) vs. profile-preference file (persisted) split described in §2.5, reinforcing that pattern rather than contradicting it.

### 3.5 Banner/startup treatment

- `banner` is a documented, user-configurable **frequency** setting with values `always`, `once`, or `never` (`.../customize-copilot/change-settings`, "Settings you might want to change": `banner — How often to show the startup banner (always, once, or never)`), i.e. GitHub's own CLI treats its banner as a static, togglable startup element rather than a persistent animation — consistent with, and a reasonable external precedent for, #93's "static… appears at top… naturally scrolls away" instruction and its removal of TASC's animated banner.

### 3.6 Non-interactive / scriptable parity

- Copilot CLI explicitly supports a non-animated, decoration-free output mode for scripting: `-s` — "Suppress stats and decoration, outputting only the agent's response. Ideal for piping output in scripts." (`docs.github.com/en/copilot/reference/copilot-cli-reference/cli-programmatic-reference`, "Command line options" table). This parallels TASC's existing `--format json`/non-TTY banner fallback (`banner.rs:59-62`, `main.rs`/`agent.rs` JSON output gating) and confirms that #93's "do not replace scriptable commands… with a full-screen-only interface" boundary matches how GitHub's own CLI is designed (interactive TUI and scriptable output are two first-class, coexisting modes, not one subsuming the other).

## 4. Synthesis: constraints the terminal-safe design must satisfy

1. **Layout** — bottom input, scrollable middle transcript, status/footer line below input (per #93) is *not yet implemented*; it is an established pattern in GitHub's own CLI as a configurable "footer" (§3.1) and should be built as a persistent, redrawn region, distinct from the current repo's one-shot `print!`/`println!` rendering (`agent.rs:1053-1322`).
2. **Color** — reuse the existing, tested `is_terminal() && !NO_COLOR && !json_output` gate (`agent.rs:1013-1019,1062-1067`; `banner.rs:34-36,57-58`) as the single color-enablement policy; do not invent a second gate. Follow GitHub's documented rule that shape/glyph, not color alone, must carry meaning, because color shifts with terminal theme/16-color fallback/colorblind themes (§3.2) — any new "state/warning/action" color coding needs a non-color-dependent fallback glyph or label, matching today's already-safe plain-text fallback branch.
3. **Icons** — greenfield: no existing semantic icon vocabulary to preserve beyond the single banner `⚡` (`banner.rs:46`); new icon choices must degrade to plain ASCII/text labels for `NO_COLOR`/non-TTY output, consistent with the existing `print_shell_event`/`shell_event_output` plain-text fallback (`agent.rs:1322-1337`).
4. **Slash-command/picker grammar** — current `parse_shell_command` is a post-Enter, non-interactive parser (§2.4); GitHub's own CLI's `/settings`-style grammar (type `/` to filter → arrow-navigate → commit key → escape to cancel, §3.3) is the concrete external reference #93 asks the design to feel "comparable in interaction quality to," applied uniformly across `/model`, `/skill`, `/evaluation`, `/consistency`, `/security`, `/command-skill`, etc., rather than one bespoke UI per command family.
5. **Model picker** — must keep TASC's existing, tested session-vs-profile distinction (`ShellCommand::ModelSession` vs. `ModelProfile`, `agent.rs:761-765,1796-1809`) as two selectable actions in the picker, not collapse them into one; the already-available `client.list_models()` call (`agent.rs:1024-1029`) is the data source the picker should render instead of the current flat joined-string print.
6. **Pagination/scroll** — net-new; no existing primitive to build on in `tasc-cli`/`tasc-agent` (§2.6); any inventory-list picker needs its own virtual-scroll/paging logic.
7. **Status line** — content is already computed and correct (`dashboard_status`/`dashboard_output`, `agent.rs:908-1011`: GitHub CLI auth, model, token usage, skill/tracked/watched counts, onboarding hint); the remaining work is purely placement (move from "prints once above the prompt" to "persistent region below input") and selective color/icon treatment per #93, not recomputing its data.
8. **Banner** — two divergent implementations exist (`banner.rs`'s 32-frame animation vs. `agent.rs`'s single static rainbow render); #93's "remove the animated banner" applies to `banner.rs`; the `agent.rs` static banner is already structurally compliant and is the better starting point to unify onto.
9. **Documentation screenshot** — no terminal screenshot file currently exists in this repository (§1); GitHub's own docs model the target end-state (a real terminal screenshot, `copilot-cli-welcome.png`, embedded directly in a concepts page, §3.1), but #93's referenced current-agent image must be attached or a replacement must be captured from the finalized prototype before README/Pages asset work proceeds.

## Gaps and uncertainties

- The current-agent screenshot referenced in #93's Notes could not be located in the repository, issue attachments, or issue comments; the intended visual reference therefore remains unavailable.
- `github/copilot-cli`'s actual TUI implementation is closed-source; all Copilot CLI claims above are sourced exclusively from `docs.github.com`, as instructed, and are therefore necessarily behavior-level (what the CLI does) rather than implementation-level (how it renders) — appropriate for an interaction/constraint baseline, but implementers should not assume any specific terminal library or layout engine is implied.
- This research did not open or modify #93, #94, or #95–#98, and made no product code changes, per instructions.

## Primary sources

### TASC

- Issues [#93](https://github.com/pascalvanderheiden/turbo-agent-skills-cli/issues/93) and [#94](https://github.com/pascalvanderheiden/turbo-agent-skills-cli/issues/94).
- [`CONTEXT.md` at the research base commit](https://github.com/pascalvanderheiden/turbo-agent-skills-cli/blob/7a1748d6a75a10a5d3c84e03daabd5c27615e803/CONTEXT.md).
- [`README.md` at the research base commit](https://github.com/pascalvanderheiden/turbo-agent-skills-cli/blob/7a1748d6a75a10a5d3c84e03daabd5c27615e803/README.md).
- [`crates/tasc-cli/src/agent.rs` at the research base commit](https://github.com/pascalvanderheiden/turbo-agent-skills-cli/blob/7a1748d6a75a10a5d3c84e03daabd5c27615e803/crates/tasc-cli/src/agent.rs).
- [`crates/tasc-cli/src/banner.rs` at the research base commit](https://github.com/pascalvanderheiden/turbo-agent-skills-cli/blob/7a1748d6a75a10a5d3c84e03daabd5c27615e803/crates/tasc-cli/src/banner.rs).
- [`docs/prototypes/tasc-agent-interaction.html` at the research base commit](https://github.com/pascalvanderheiden/turbo-agent-skills-cli/blob/7a1748d6a75a10a5d3c84e03daabd5c27615e803/docs/prototypes/tasc-agent-interaction.html).

### GitHub Copilot CLI

- [GitHub Copilot CLI concepts](https://docs.github.com/en/copilot/concepts/copilot-surfaces/copilot-cli).
- [GitHub Copilot CLI command reference](https://docs.github.com/en/copilot/reference/copilot-cli-reference/cli-command-reference).
- [Changing settings with `/settings`](https://docs.github.com/en/copilot/how-tos/copilot-cli/customize-copilot/change-settings).
- [Using GitHub Copilot CLI](https://docs.github.com/en/copilot/how-tos/copilot-cli/use-copilot-cli/overview).
- [GitHub Copilot CLI programmatic reference](https://docs.github.com/en/copilot/reference/copilot-cli-reference/cli-programmatic-reference).
- [Official `github/copilot-cli` repository](https://github.com/github/copilot-cli).
