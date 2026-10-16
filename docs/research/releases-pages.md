# Research: GitHub Releases and Pages Delivery Patterns

**Issue:** [#5 Research GitHub Releases and Pages delivery patterns](https://github.com/pascalvanderheiden/turbo-agent-skills-cli/issues/5)
**Part of:** [#1 TASC: implementation-ready product specification and roadmap](https://github.com/pascalvanderheiden/turbo-agent-skills-cli/issues/1)
**Boundary from #2:** Initial distribution is GitHub Releases binaries for macOS, Linux, and Windows plus GitHub Pages documentation. Native package managers (Homebrew, Scoop, `cargo install`/crates.io, apt/dnf, winget, etc.) are explicitly **later scope** and are out of bounds for this research.

## Question

What GitHub-native release, provenance, cross-platform binary, and GitHub Pages practices should constrain TASC's initial distribution design for macOS, Linux, and Windows?

---

## Executive recommendation

For a local-first Rust CLI shipped only through GitHub Releases + GitHub Pages, adopt this minimal-but-credible baseline:

1. **Tag-triggered release workflow.** A pushed `v*` tag (annotated, SemVer 2.0.0) triggers a GitHub Actions workflow matrix that builds five Tier 1 targets on matching x64/ARM64 macOS, Linux, and Windows hosted runners, packages them, produces checksums and an SBOM, and generates signed attestations before publishing one draft-then-published GitHub Release. Add `x86_64-unknown-linux-musl` after the baseline pipeline is stable. Drafting first ensures all assets land before the release becomes immutable ([GitHub Docs: Managing releases in a repository](https://docs.github.com/en/repositories/releasing-projects-on-github/managing-releases-in-a-repository); [GitHub Docs: Immutable releases](https://docs.github.com/en/code-security/concepts/supply-chain-security/immutable-releases); [GitHub Docs: GitHub-hosted runners reference](https://docs.github.com/en/actions/reference/runners/github-hosted-runners)).
2. **Supply-chain evidence as a first-class release artifact**, not an afterthought: per-asset SHA-256 checksums file, a CycloneDX or SPDX SBOM, and a GitHub Artifact Attestation (SLSA-aligned build provenance, Sigstore-backed, keyless) for every binary, generated with the first-party `actions/attest` action and verifiable with `gh attestation verify` ([GitHub Docs: Using artifact attestations to establish provenance for builds](https://docs.github.com/en/actions/how-tos/secure-your-work/use-artifact-attestations/use-artifact-attestations)).
3. **Enable immutable releases and a tag-protecting ruleset** on the repository so published tags/assets cannot be silently altered, and treat "fixing" a bad release as **yanking + re-releasing under a new version**, never mutating a published tag ([GitHub Docs: Immutable releases](https://docs.github.com/en/code-security/concepts/supply-chain-security/immutable-releases); [SemVer 2.0.0 §3](https://semver.org/)).
4. **GitHub Pages via the Actions deployment flow** (`actions/upload-pages-artifact` + `actions/deploy-pages`) rather than branch-based publishing, so docs can be generated from source (e.g., mdBook/rustdoc) and deployed under restrictive OIDC-scoped permissions without a dedicated `gh-pages` branch ([GitHub Docs: Configuring a publishing source for your GitHub Pages site](https://docs.github.com/en/pages/getting-started-with-github-pages/configuring-a-publishing-source-for-your-github-pages-site); [actions/deploy-pages](https://github.com/actions/deploy-pages)).
5. **Defer OS-native code signing for v1.** Apple notarization requires Developer ID signing, and Windows trust controls expect a certificate from a trusted provider or Microsoft's Artifact Signing service. These identity, certificate, and platform-specific release operations are separate from GitHub provenance and can be added later if unsigned-binary friction warrants them. For v1, clearly state that GitHub attestations prove workflow origin and integrity but do not replace Gatekeeper or Windows publisher trust ([Apple Developer: Developer ID and notarization](https://developer.apple.com/developer-id/); [Microsoft Learn: Sign your app for Smart App Control compliance](https://learn.microsoft.com/en-us/windows/apps/develop/smart-app-control/code-signing-for-smart-app-control)).

This keeps the v1 surface entirely GitHub-native (Releases + Actions + Pages), defers package-manager-specific packaging, and establishes signed SLSA build provenance without requiring TASC to operate long-lived signing keys of its own.

---

## 1. Target matrix and asset naming

### 1.1 Platform/architecture matrix

Rust's own platform support is tiered; **Tier 1** targets have guaranteed, tested host toolchains and are the right v1 baseline, with **Tier 2** `musl` and `aarch64` additions covering the rest of the realistic user base ([rustc Platform Support](https://doc.rust-lang.org/rustc/platform-support.html)):

| OS | Arch | Rust target triple | Tier | Notes |
|---|---|---|---|---|
| Linux | x86_64 | `x86_64-unknown-linux-gnu` | 1 | Primary Linux target; requires glibc on host |
| Linux | x86_64 (static) | `x86_64-unknown-linux-musl` | 2 | Fully static binary; avoids glibc version skew across distros |
| Linux | aarch64 | `aarch64-unknown-linux-gnu` | 1 | Covers ARM servers/dev boxes (e.g., Graviton, Raspberry Pi 64-bit, Apple Silicon Linux VMs) |
| macOS | x86_64 | `x86_64-apple-darwin` | 1 | Intel Macs |
| macOS | aarch64 | `aarch64-apple-darwin` | 1 | Apple Silicon |
| Windows | x86_64 | `x86_64-pc-windows-msvc` | 1 | MSVC ABI is the Rust-recommended default for Windows distribution |

Use GitHub-hosted runners matching both target OS and architecture: `ubuntu-latest` for Linux x64, `ubuntu-24.04-arm` for Linux ARM64, `macos-15-intel` for Intel macOS, `macos-15` (or the then-current ARM64 label) for Apple Silicon, and `windows-latest` for Windows x64. GitHub currently exposes standard ARM64 Linux and macOS runners for public repositories, so the five Tier 1 targets do not require architecture cross-linking ([GitHub Docs: GitHub-hosted runners reference](https://docs.github.com/en/actions/reference/runners/github-hosted-runners)). `x86_64-unknown-linux-musl` can build on Linux x64 after installing the Rust target and required musl linker tooling ([The Cargo Book: Continuous Integration](https://doc.rust-lang.org/cargo/guide/continuous-integration.html); [rustc Platform Support](https://doc.rust-lang.org/rustc/platform-support.html)).

### 1.2 Asset naming convention

Adopt a convention that encodes project, version, target triple, and extension explicitly — this is the de facto GitHub-ecosystem pattern (used by `ripgrep`, `rustup`, `fd`, etc., all of which build on the same Rust target-triple vocabulary) and keeps install scripts parseable without special-casing:

```
tasc-v{version}-{target-triple}.{ext}
tasc-v0.1.0-x86_64-unknown-linux-gnu.tar.gz
tasc-v0.1.0-x86_64-unknown-linux-musl.tar.gz
tasc-v0.1.0-aarch64-unknown-linux-gnu.tar.gz
tasc-v0.1.0-x86_64-apple-darwin.tar.gz
tasc-v0.1.0-aarch64-apple-darwin.tar.gz
tasc-v0.1.0-x86_64-pc-windows-msvc.zip
```

- **Archive format:** `.tar.gz` for Unix-like targets, `.zip` for Windows (matches platform tooling conventions and avoids requiring `tar` on a stock Windows shell for older Windows versions).
- **Checksums file:** a single `checksums.txt` (or `SHA256SUMS`) listing `<sha256-hex>  filename` for every asset, generated with the standard `shasum -a 256`/`sha256sum`/`Get-FileHash` tooling per OS, attached to the same release.
- **SBOM file(s):** one SBOM per binary or one SBOM per release covering all binaries, named `tasc-v{version}.cdx.json` (CycloneDX) or `tasc-v{version}.spdx.json` (SPDX) — pick one format (see §3.3) rather than shipping both, to avoid consumer confusion.
- **Attestations** are not uploaded as release assets; they are published to the repository's attestation store and fetched/verified by digest via `gh attestation verify`, so no separate filename convention is needed ([GitHub Docs: Using artifact attestations](https://docs.github.com/en/actions/how-tos/secure-your-work/use-artifact-attestations/use-artifact-attestations)).

GitHub Releases already provides free source-code `zip`/`tarball` assets automatically per tag; the matrix above is *additive* binary distribution, not a replacement ([GitHub Docs: Managing releases in a repository](https://docs.github.com/en/repositories/releasing-projects-on-github/managing-releases-in-a-repository)).

---

## 2. Workflow architecture (GitHub Actions)

### 2.1 Trigger and job shape

- **Trigger:** `push: tags: ['v*']` — the tag is the single source of truth for "this is a release," matching GitHub's own documented release flow of creating/choosing a tag first ([GitHub Docs: Managing releases in a repository](https://docs.github.com/en/repositories/releasing-projects-on-github/managing-releases-in-a-repository)).
- **Matrix build job:** one job per (OS, target) pair using `strategy.matrix`, each building the release binary natively or via a documented cross target, running tests on at least the native targets, then producing the archive + per-asset checksum.
- **Aggregate/release job:** depends on all matrix legs; downloads all build artifacts via `actions/upload-artifact`/`download-artifact`, concatenates the checksums file, generates the SBOM, runs `actions/attest` for provenance (and optionally `sbom-path` for an SBOM attestation), and then creates the release.
- **Release publication:** use `gh release create` (GitHub CLI, already a required foundation for TASC per issue #1/#2) in **draft-first** mode, attach every asset, then transition to published — this is GitHub's own recommended sequencing for immutable releases so that all assets exist before the tag/assets lock ([GitHub Docs: Immutable releases — "Best practices for publishing immutable releases"](https://docs.github.com/en/code-security/concepts/supply-chain-security/immutable-releases); [GitHub CLI manual: `gh release create`](https://cli.github.com/manual/gh_release_create)).
- **Workflow hardening:** give build jobs `contents: read` only and grant release/attestation permissions only to the aggregate job; validate that the tag version equals `Cargo.toml` before building; use a protected `release` environment restricted to `v*` tags; and lock action dependencies with a workflow dependency lockfile or, where unavailable, full-length commit SHAs. GitHub identifies lockfiles as the strongest protection and full-SHA pinning as the fallback ([GitHub Docs: Secure use reference](https://docs.github.com/en/actions/reference/security/secure-use); [GitHub Docs: Deployments and environments](https://docs.github.com/en/actions/reference/workflows-and-actions/deployments-and-environments)).

### 2.2 Minimal permissions

Each job should request only what it needs, following GitHub's principle of least privilege for `GITHUB_TOKEN`:

Set permissions per job rather than workflow-wide: build jobs need only `contents: read`; the aggregate release job needs `contents: write`, `id-token: write`, and `attestations: write`.

`id-token: write` is the OIDC grant that lets `actions/attest` (and Sigstore/cosign keyless flows generally) mint a short-lived certificate bound to the workflow's identity without any stored signing key ([GitHub Docs: About security hardening with OpenID Connect](https://docs.github.com/en/actions/concepts/security/openid-connect); [Sigstore Docs: Cosign signing overview](https://docs.sigstore.dev/cosign/signing/overview/)).

### 2.3 Illustrative workflow skeleton

```yaml
name: release
on:
  push:
    tags: ['v*']

jobs:
  build:
    permissions:
      contents: read
    strategy:
      matrix:
        include:
          - os: ubuntu-latest
            target: x86_64-unknown-linux-gnu
          - os: ubuntu-24.04-arm
            target: aarch64-unknown-linux-gnu
          - os: macos-15
            target: aarch64-apple-darwin
          - os: macos-15-intel
            target: x86_64-apple-darwin
          - os: windows-latest
            target: x86_64-pc-windows-msvc
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@<full-commit-sha>
      - run: rustup target add ${{ matrix.target }}
      - run: cargo build --release --locked --target ${{ matrix.target }}
      - run: # package archive + sha256 per matrix leg
      - uses: actions/upload-artifact@<full-commit-sha>
        with:
          name: dist-${{ matrix.target }}
          path: dist/*

  release:
    needs: build
    runs-on: ubuntu-latest
    environment: release
    permissions:
      contents: write
      id-token: write
      attestations: write
    env:
      GH_TOKEN: ${{ github.token }}
      GH_REPO: ${{ github.repository }}
    steps:
      - uses: actions/download-artifact@<full-commit-sha>
      - run: # merge checksums.txt, generate SBOM
      - uses: actions/attest@<full-commit-sha>
        with:
          subject-checksums: 'dist/checksums.txt'
      - uses: actions/attest@<full-commit-sha>
        with:
          subject-checksums: 'dist/checksums.txt'
          sbom-path: 'dist/tasc.cdx.json'
      - run: gh release create "$GITHUB_REF_NAME" dist/** --draft --title "$GITHUB_REF_NAME" --generate-notes
      - run: gh release edit "$GITHUB_REF_NAME" --draft=false
```

(Illustrative baseline only — add the musl matrix leg in Phase 2. Resolve each action placeholder to a reviewed current release commit, record it in the workflow dependency lockfile where available, and let Dependabot maintain updates. The two `actions/attest` invocations are intentional: one emits build provenance and one emits an SBOM attestation; supplying `sbom-path` switches the action to SBOM mode ([actions/attest: Attestation modes](https://github.com/actions/attest#attestation-modes)).)

### 2.4 Release notes generation

GitHub's native **auto-generated release notes** (`--generate-notes` / "Generate release notes" in the UI) assembles notes from merged PR titles and new contributors between the previous and current tag, and is the documented, zero-maintenance default; teams can prepend manual highlights via `--notes` ([GitHub Docs: Managing releases in a repository](https://docs.github.com/en/repositories/releasing-projects-on-github/managing-releases-in-a-repository); [GitHub CLI manual: `gh release create`](https://cli.github.com/manual/gh_release_create)). Recommend using auto-generated notes as the baseline, with a short hand-written summary of behavior changes prepended for human readability.

---

## 3. Security / supply-chain approach

### 3.1 Checksums

A `sha256` checksums file is the minimum bar: it lets any consumer verify bit-for-bit integrity offline without trusting GitHub's transport layer alone. GitHub's own supply-chain docs point to this as the baseline verification mechanism, complemented by attestations for authenticity (who built it) rather than just integrity (did it change) ([GitHub Docs: Verifying the integrity of a release](https://docs.github.com/en/code-security/how-tos/secure-your-supply-chain/secure-your-dependencies/verify-release-integrity)).

### 3.2 Artifact attestations (GitHub-native, SLSA/Sigstore-backed)

GitHub Artifact Attestations are the recommended first-party mechanism for establishing **build provenance**:

- Generated with `actions/attest` using `id-token: write` + `attestations: write` permissions, pointed at the built binary via `subject-path` ([GitHub Docs: Using artifact attestations to establish provenance for builds](https://docs.github.com/en/actions/how-tos/secure-your-work/use-artifact-attestations/use-artifact-attestations)).
- Verifiable by any consumer with `gh attestation verify PATH/TO/BINARY -R owner/repo`, including offline verification paths for air-gapped environments ([same page, "Verifying artifact attestations with the GitHub CLI"](https://docs.github.com/en/actions/how-tos/secure-your-work/use-artifact-attestations/use-artifact-attestations)).
- Under the hood this is **keyless signing** via Sigstore: Fulcio issues a short-lived certificate bound to the GitHub Actions OIDC identity, the signing event is recorded in the Rekor transparency log, and the private key is discarded immediately — so there is no long-lived signing key for TASC to protect or rotate ([Sigstore Docs: Cosign signing overview](https://docs.sigstore.dev/cosign/signing/overview/)).
- This combination (hosted build platform + signed, non-forgeable provenance) maps to **SLSA Build Level 2**, and with a hardened/isolated build (ephemeral GitHub-hosted runners, no self-hosted shared state) trends toward **Build Level 3** — SLSA's levels measure exactly this axis: provenance existence (L1) → signed provenance from a hosted platform (L2) → tamper-resistant hardened build platform (L3) ([SLSA Spec v1.0](https://slsa.dev/spec/v1.0/); [SLSA Spec v1.0: Levels](https://slsa.dev/spec/v1.0/levels)).

### 3.3 SBOM

Generate a machine-readable SBOM per release and attach it both as a release asset and as an **SBOM attestation** (`actions/attest` with `sbom-path`), which lets consumers cryptographically verify the SBOM itself is attributable to the same build, not just trust a loose file ([GitHub Docs: Using artifact attestations — "Generating an attestation for a software bill of materials (SBOM)"](https://docs.github.com/en/actions/how-tos/secure-your-work/use-artifact-attestations/use-artifact-attestations)).

Two first-party-maintained formats are viable:

- **CycloneDX**: purpose-built object model spanning components, dependencies, vulnerabilities (VEX), and *formulation* (build/deployment provenance description) — strong fit for a security-focused supply-chain use case ([CycloneDX Specification Overview](https://cyclonedx.org/specification/overview/)).
- **SPDX**: ISO/IEC 5962-standardized, broader ecosystem/legal-compliance orientation, SPDX 3.0 generalizes beyond traditional software composition ([SPDX overview](https://spdx.dev/learn/overview/)).

**Recommendation:** CycloneDX JSON as the primary SBOM format for TASC's binaries, because CycloneDX's Rust/`cargo`-ecosystem tooling (e.g., `cargo-cyclonedx`-style generators consuming `Cargo.lock`) is purpose-fit for a single-crate-workspace Rust CLI and the format's native vulnerability/VEX fields are directly useful for TASC's own future "Evaluation" and "Consistency Check" capabilities (per `CONTEXT.md`). SPDX remains a reasonable alternative if downstream consumers standardize on it; the two are not mutually exclusive but **shipping one well beats shipping both poorly** for v1.

### 3.4 Signing beyond attestations (code signing) — explicitly deferred

- **macOS**: Apple describes notarization as scanning Developer ID-signed software and issuing a ticket that Gatekeeper can recognize. That requires Apple-specific identity and release steps beyond GitHub Actions provenance, so defer it while documenting that the v1 binary is not notarized ([Apple Developer: Developer ID and notarization](https://developer.apple.com/developer-id/)).
- **Windows**: Microsoft says Smart App Control trusts apps signed with a valid certificate from a trusted provider and identifies Artifact Signing as its preferred signing path. This is likewise a distinct platform trust workflow, so defer it while documenting that the v1 binary has no Windows publisher signature ([Microsoft Learn: Sign your app for Smart App Control compliance](https://learn.microsoft.com/en-us/windows/apps/develop/smart-app-control/code-signing-for-smart-app-control)).
- **Sigstore/cosign for the raw binaries** (as opposed to GitHub's managed attestation flow) is a viable *alternative* if TASC ever needs attestation verification independent of GitHub's hosted service, since `cosign`'s keyless flow is the same Fulcio/Rekor mechanism GitHub's `actions/attest` builds on ([Sigstore Docs: Cosign signing overview](https://docs.sigstore.dev/cosign/signing/overview/)) — but using the first-party `actions/attest`/`gh attestation verify` pair is simpler for a GitHub-native v1 and avoids operating separate tooling.

**Net effect:** v1 ships **unsigned-by-OS-standards but attested-by-GitHub** binaries. This is an explicit, documented risk (see §6) rather than a silent gap.

### 3.5 Tag and release protections

- Enable **immutable releases** at the repository (or org) level: once published, the release's Git tag is locked to its commit and cannot move or be deleted while the release exists, and release assets cannot be modified or deleted — this directly blocks the classic "attacker swaps the binary behind an existing tag" supply-chain attack, and also blocks "repository resurrection" (recreating a deleted repo to reuse a tag name) ([GitHub Docs: Immutable releases](https://docs.github.com/en/code-security/concepts/supply-chain-security/immutable-releases)).
- Complement this with a **tag ruleset** (`Restrict creations` / `Restrict updates` / `Restrict deletions` for the `v*` pattern) so that even non-release tag mutation is bypass-gated to authorized actors, and consider `Require signed commits` for the default branch to raise the bar on the commits a tag can point to ([GitHub Docs: Available rules for rulesets](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-rulesets/available-rules-for-rulesets)).
- Publishing an immutable release also auto-generates a **release attestation** — a cryptographically verifiable record binding the release tag, commit SHA, and asset list — on top of the per-binary artifact attestations, giving consumers a release-level integrity anchor as well as a binary-level one ([GitHub Docs: Immutable releases](https://docs.github.com/en/code-security/concepts/supply-chain-security/immutable-releases)).

---

## 4. Versioning, rollback, and re-release

### 4.1 SemVer as the versioning contract

Adopt **Semantic Versioning 2.0.0** (`MAJOR.MINOR.PATCH[-PRERELEASE][+BUILD]`) as the public contract for TASC CLI releases: MAJOR for incompatible CLI/API changes, MINOR for backward-compatible functionality, PATCH for backward-compatible fixes, with `0.y.z` reserved for initial development where "anything MAY change at any time" ([SemVer 2.0.0](https://semver.org/)). This aligns with Rust/Cargo's own ecosystem-standard compatibility rules for crates, which TASC should mirror for consistency even though it is not publishing to crates.io yet, since `cargo`'s SemVer compatibility guidance is the dominant convention Rust developers already expect ([The Cargo Book: SemVer Compatibility](https://doc.rust-lang.org/cargo/reference/semver.html)).

Critically, SemVer Rule 3 states: **"Once a versioned package has been released, the contents of that version MUST NOT be modified. Any modifications MUST be released as a new version"** ([SemVer 2.0.0 §3](https://semver.org/)). This is the normative justification for combining SemVer with GitHub's immutable releases — the spec already forbids mutating a released version; immutable releases make that technically unavoidable rather than merely a social convention.

### 4.2 Tag practices

- Use **annotated tags** (`git tag -a vX.Y.Z -m "..."`) pushed to the default branch's release commit; `gh release create --notes-from-tag` can then source release notes from the tag's annotation message, giving a single source of truth for "why this version" ([GitHub CLI manual: `gh release create`](https://cli.github.com/manual/gh_release_create)).
- Tag format: `vX.Y.Z` (leading `v`, matching the ubiquitous Rust/GitHub-ecosystem convention and GitHub's own UI/CLI examples) ([GitHub Docs: Managing releases in a repository](https://docs.github.com/en/repositories/releasing-projects-on-github/managing-releases-in-a-repository) — tag naming examples use `v1.3.2`).
- Pre-releases: use SemVer pre-release identifiers (`v1.0.0-rc.1`) and GitHub's **prerelease** flag (`--prerelease`) so they're excluded from "Set as latest" / `gh release download` latest-resolution by default ([GitHub Docs: Managing releases in a repository](https://docs.github.com/en/repositories/releasing-projects-on-github/managing-releases-in-a-repository); [GitHub CLI manual: `gh release create`](https://cli.github.com/manual/gh_release_create)).

### 4.3 Rollback and "yanking" tradeoffs

Because immutable releases forbid deleting assets or moving tags once published, **"fixing" a bad release has exactly two legitimate options**, both of which should be documented in TASC's contributor docs:

1. **Mark the bad release as a pre-release / not-latest** and publish a new PATCH version that supersedes it. `gh release edit` can still change the title, notes, and the prerelease/latest flags on an immutable release (those fields are explicitly *not* locked) ([GitHub Docs: Immutable releases — "Only the assets and tag are locked"](https://docs.github.com/en/code-security/concepts/supply-chain-security/immutable-releases)). This is the **recommended default**: it preserves the audit trail (nothing disappears), keeps attestations valid, and matches the SemVer expectation that a new defect fix is a new version.
2. **Delete the entire release** (not just assets) if it must stop being installable at all — this is the only way to free the tag name, and even then the tag cannot be *reused* for immutable releases due to the repository-resurrection protection, so the practical outcome is "delete and move on to the next version number," not "delete and retry the same tag" ([GitHub Docs: Immutable releases](https://docs.github.com/en/code-security/concepts/supply-chain-security/immutable-releases)).

**Recommended decision for TASC:** never attempt to overwrite or "re-publish" a given tag. Treat every published release as permanent; ship fixes as new PATCH (or MINOR/MAJOR as appropriate) versions, and use "pre-release"/"latest" flag changes plus clear release-notes callouts ("⚠️ superseded by vX.Y.Z — do not use") as the rollback mechanism. This mirrors how crates.io's "yank" works conceptually (mark unusable, don't delete history) even though TASC is not on crates.io yet.

### 4.4 Verifying integrity end-to-end

Document a consumer-facing verification path combining all of the above:

```bash
gh release verify vX.Y.Z                              # confirms release + tag are immutable
gh release verify-asset vX.Y.Z tasc-vX.Y.Z-<target>.tar.gz   # confirms local file matches published asset
gh attestation verify tasc-vX.Y.Z-<target>.tar.gz -R pascalvanderheiden/turbo-agent-skills-cli  # confirms build provenance
sha256sum -c checksums.txt                              # confirms integrity against the published checksums file
```

(`gh release verify`/`verify-asset` per [GitHub Docs: Verifying the integrity of a release](https://docs.github.com/en/code-security/how-tos/secure-your-supply-chain/secure-your-dependencies/verify-release-integrity); `gh attestation verify` per [GitHub Docs: Using artifact attestations](https://docs.github.com/en/actions/how-tos/secure-your-work/use-artifact-attestations/use-artifact-attestations).)

---

## 5. GitHub Pages documentation delivery

### 5.1 Publishing source: Actions workflow, not a branch

GitHub Pages supports two publishing sources — a designated branch/folder, or a GitHub Actions workflow that uploads a build artifact. GitHub's own guidance: use branch-based publishing only "if you do not need any control over the build process," and use an **Actions workflow** "if you want to use a build process other than Jekyll" ([GitHub Docs: Configuring a publishing source for your GitHub Pages site](https://docs.github.com/en/pages/getting-started-with-github-pages/configuring-a-publishing-source-for-your-github-pages-site)). Since TASC's docs will likely be generated (mdBook, rustdoc, or a static-site generator) rather than hand-authored Jekyll, the Actions path is the correct choice and avoids maintaining a separate `gh-pages` branch with force-pushed history.

### 5.2 Workflow shape

Two-job pattern, matching GitHub's documented starter workflows:

1. **Build job**: generates the static site (e.g., `mdbook build`, `cargo doc`, or equivalent) and uploads it with `actions/upload-pages-artifact`.
2. **Deploy job**: depends on build, runs under the `github-pages` environment, requests `permissions: pages: write` and `id-token: write` (the latter so GitHub can verify the deployment's origin), and calls `actions/deploy-pages` to publish ([actions/deploy-pages README](https://github.com/actions/deploy-pages)).

```yaml
permissions:
  pages: write
  id-token: write

environment:
  name: github-pages
  url: ${{ steps.deployment.outputs.page_url }}

steps:
  - id: deployment
    uses: actions/deploy-pages@v4
```

### 5.3 Project-site URL shape and triggers

TASC's repository is a project (not a `<owner>.github.io` user/org) repository, so its Pages site defaults to `https://pascalvanderheiden.github.io/turbo-agent-skills-cli/` under GitHub's documented project-site location rules, with a maximum of one Pages site per repository ([GitHub Docs: What is GitHub Pages?](https://docs.github.com/en/pages/getting-started-with-github-pages/what-is-github-pages)). Trigger the docs workflow on pushes to the default branch (for "docs track main") and optionally on the same `v*` tag used for releases (to publish a versioned docs snapshot alongside each release) — the latter is a design choice to make during implementation, not a hard requirement from GitHub's docs.

### 5.4 Privacy note

GitHub Pages sites are public on the internet by default even if the source repository is private (where the plan allows private Pages) — since TASC's repository is public this is moot, but it's worth keeping in mind if any private-preview documentation is ever hosted this way ([GitHub Docs: Configuring a publishing source for your GitHub Pages site — "Pages sites are publicly available"](https://docs.github.com/en/pages/getting-started-with-github-pages/configuring-a-publishing-source-for-your-github-pages-site)).

---

## 6. Risks and tradeoffs

| Risk / tradeoff | Detail | Mitigation / disposition |
|---|---|---|
| **Unsigned OS-level binaries** | GitHub provenance does not provide Apple notarization or a Windows trusted-publisher signature | State the limitation explicitly; provide checksum and attestation verification; revisit native signing if adoption data shows platform trust prompts block users |
| **Immutable releases lock in mistakes** | A bad release (wrong binary, leaked secret in notes, etc.) cannot have its assets patched in place | Accept as a feature, not a bug: enforces "ship a new version" discipline; use prerelease/latest flags and release-notes warnings for rollback messaging |
| **Matrix build complexity/cost** | 6 target legs × signing/attestation/SBOM steps increases workflow maintenance surface and Actions minutes | Start with the Tier 1 subset (5 targets) and add `musl` once the pipeline is stable; GitHub-hosted runners are free for public repos |
| **SBOM format lock-in** | Choosing CycloneDX now creates switching cost if downstream tooling later standardizes on SPDX | Keep SBOM generation in an isolated workflow step so the format can be swapped without touching the build/signing steps |
| **No package-manager reach (by design)** | Users without GitHub familiarity must manually download/verify binaries; no `brew install`/`winget install` in v1 | Explicitly deferred per issue #2's delivery boundary; revisit as a separate roadmap item once GitHub Releases + Pages is stable |
| **Attestation trust is GitHub-specific** | `gh attestation verify` and the attestation store are tied to GitHub as the verification authority | Acceptable for a GitHub-native v1; Sigstore's underlying Fulcio/Rekor infrastructure is the same substrate `cosign` uses, so a future migration path to self-verified cosign signatures remains open if needed |
| **Target correctness** | A successful compile does not prove behavior on every supported OS/architecture | Use native x64/ARM64 hosted runners for the five Tier 1 builds; run target-native smoke tests before packaging; add musl-specific tests when that Tier 2 asset is introduced |

---

## 7. Phased implementation plan

1. **Phase 0 — Baseline release workflow.** Tag-triggered build matrix for the 5 Tier 1 targets on matching hosted-runner architectures, `gh release create` with auto-generated notes, SHA-256 checksums file. No attestations/SBOM yet. Validates the core pipeline end-to-end.
2. **Phase 1 — Supply-chain hardening.** Add `actions/attest` artifact attestations, CycloneDX SBOM + SBOM attestation, enable immutable releases and a `v*` tag ruleset at the repository level.
3. **Phase 2 — Broaden target matrix.** Add `x86_64-unknown-linux-musl` and `aarch64-unknown-linux-gnu`; add cross-architecture test validation.
4. **Phase 3 — GitHub Pages documentation site.** Stand up the Actions-based Pages workflow (`upload-pages-artifact` + `deploy-pages`) for generated docs (mdBook/rustdoc), triggered on default-branch pushes; optionally snapshot docs per release tag.
5. **Phase 4 — Verification UX.** Publish a documented, copy-pasteable verification recipe (§4.4) in the repository README/docs so users can confirm checksum, attestation, and immutability status before running an installed binary.
6. **Out of scope for this roadmap (tracked separately):** native package manager packaging (Homebrew tap, Scoop bucket, winget manifest, crates.io publishing, Linux distro packaging) and paid OS code-signing/notarization.

---

## 8. Concrete recommended decisions for this repository

- **Tag format:** `vMAJOR.MINOR.PATCH` annotated tags; pre-releases as `vMAJOR.MINOR.PATCH-rc.N`.
- **Versioning scheme:** SemVer 2.0.0, starting at `0.y.z` during initial development per SemVer §4, graduating to `1.0.0` once the CLI's public command/flag surface is declared stable.
- **Target matrix (v1):** `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`, `x86_64-apple-darwin`, `aarch64-apple-darwin`, `x86_64-pc-windows-msvc`; add `x86_64-unknown-linux-musl` in Phase 2.
- **Asset naming:** `tasc-v{version}-{target-triple}.{tar.gz|zip}`, with a release-wide `checksums.txt` (SHA-256) and `tasc-v{version}.cdx.json` CycloneDX SBOM.
- **Release process:** `gh release create` in `--draft` mode, attach all matrix assets + checksums + SBOM, generate attestations via `actions/attest` before publishing, then flip to published — never edit assets or move the tag after publish.
- **Repository settings:** enable **immutable releases** and a **tag ruleset** restricting creation/update/deletion of `v*` tags to authorized bypass actors.
- **Rollback policy:** no asset/tag mutation ever; bad releases are superseded by a new version and marked non-latest/pre-release with an explicit warning in the release notes.
- **Signing:** rely on GitHub Artifact Attestations (Sigstore-backed, keyless) as the provenance mechanism for v1; explicitly defer Apple notarization and Windows Authenticode signing.
- **SBOM format:** CycloneDX JSON, generated and attested per release.
- **Docs delivery:** GitHub Pages via `actions/upload-pages-artifact` + `actions/deploy-pages`, built from generated documentation sources, deployed from the default branch.
- **Explicitly out of scope for this effort:** Homebrew/Scoop/winget/crates.io/apt-dnf packaging — tracked as later roadmap work per issue #1/#2's delivery boundary.

---

## Key sources consulted

- GitHub Docs — [Managing releases in a repository](https://docs.github.com/en/repositories/releasing-projects-on-github/managing-releases-in-a-repository)
- GitHub Docs — [Immutable releases](https://docs.github.com/en/code-security/concepts/supply-chain-security/immutable-releases)
- GitHub Docs — [Verifying the integrity of a release](https://docs.github.com/en/code-security/how-tos/secure-your-supply-chain/secure-your-dependencies/verify-release-integrity)
- GitHub Docs — [Using artifact attestations to establish provenance for builds](https://docs.github.com/en/actions/how-tos/secure-your-work/use-artifact-attestations/use-artifact-attestations)
- GitHub Docs — [About security hardening with OpenID Connect](https://docs.github.com/en/actions/concepts/security/openid-connect)
- GitHub Docs — [GitHub-hosted runners reference](https://docs.github.com/en/actions/reference/runners/github-hosted-runners)
- GitHub Docs — [Secure use reference](https://docs.github.com/en/actions/reference/security/secure-use)
- GitHub Docs — [Deployments and environments](https://docs.github.com/en/actions/reference/workflows-and-actions/deployments-and-environments)
- GitHub Docs — [Available rules for rulesets](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-rulesets/available-rules-for-rulesets)
- GitHub Docs — [What is GitHub Pages?](https://docs.github.com/en/pages/getting-started-with-github-pages/what-is-github-pages)
- GitHub Docs — [Configuring a publishing source for your GitHub Pages site](https://docs.github.com/en/pages/getting-started-with-github-pages/configuring-a-publishing-source-for-your-github-pages-site)
- GitHub — [actions/deploy-pages](https://github.com/actions/deploy-pages)
- GitHub CLI manual — [`gh release create`](https://cli.github.com/manual/gh_release_create)
- Rust — [rustc Platform Support](https://doc.rust-lang.org/rustc/platform-support.html)
- Rust — [The Cargo Book: SemVer Compatibility](https://doc.rust-lang.org/cargo/reference/semver.html)
- Rust — [The Cargo Book: Continuous Integration](https://doc.rust-lang.org/cargo/guide/continuous-integration.html)
- [Semantic Versioning 2.0.0](https://semver.org/)
- Sigstore Docs — [Cosign signing overview](https://docs.sigstore.dev/cosign/signing/overview/)
- SLSA — [Specification v1.0](https://slsa.dev/spec/v1.0/) and [Levels](https://slsa.dev/spec/v1.0/levels)
- CycloneDX — [Specification Overview](https://cyclonedx.org/specification/overview/)
- SPDX — [Overview](https://spdx.dev/learn/overview/)
- Apple Developer — [Developer ID and notarization](https://developer.apple.com/developer-id/)
- Microsoft Learn — [Sign your app for Smart App Control compliance](https://learn.microsoft.com/en-us/windows/apps/develop/smart-app-control/code-signing-for-smart-app-control)

## Gaps and follow-ups

- Exact action commits intentionally remain implementation-time choices: review the then-current release, pin its full commit SHA (or use a workflow dependency lockfile), and keep it updated with Dependabot.
- Did not research Rust-ecosystem SBOM generator tooling (e.g., `cargo-cyclonedx`, `cargo-sbom`) in depth; this is an implementation detail for Phase 1, not a primary-source policy question.
- Native package manager research (Homebrew/Scoop/winget/crates.io) was explicitly excluded per the issue's boundary and is not covered here.
- Did not independently verify organization-vs-repository scope differences for enabling immutable releases/rulesets (GitHub Docs references both; exact settings UI should be confirmed during implementation).
