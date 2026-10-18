# Release runbook

## One-time repository configuration

1. Create a repository ruleset for tag names `v*`. Restrict tag creation to release maintainers and block tag updates and deletions. The release workflow checks GitHub's `ref_protected` context and rejects an unprotected tag.
2. Create the `release-approval` Actions environment. Require designated maintainers as reviewers and prevent the initiator from approving their own deployment. The publish job cannot start until all build, test, archive, checksum, and SBOM gates pass and an authorized reviewer approves it.
3. In repository **Settings → General → Releases**, enable **Enable release immutability**. Published release assets and their tag cannot be changed or deleted.
4. In the `release-approval` environment, set `TASC_IMMUTABLE_RELEASES_ENABLED=true` only after enabling that repository setting. The publish job requires this explicit configuration and verifies the API's immutable status after publishing; if GitHub reports a mutable release or status cannot be confirmed, it attempts to return the release to draft and fails. Manually verify and correct the release if the rollback also fails.
5. Allow GitHub Actions to create attestations with the workflow's short-lived OIDC identity. No long-lived signing secret is required.
6. Enable Pages with GitHub Actions as the build and deployment source.

## Stable release checklist

1. Update the `tasc` package version, `Cargo.lock`, `skills/tasc-command/manifest.json`, `skills/tasc-command/SKILL.md`, and `docs/site/versions.json`; add the matching `docs/site/v<version>/` documentation snapshot. Keep package and documentation versions equal.
2. Merge the complete, tested change set. Create a protected `vMAJOR.MINOR.PATCH` tag at that commit; prerelease tags are not accepted by the stable release workflow.
3. Confirm all five target builds, workspace tests, reproducible CLI and Command Skill archive packaging, the CycloneDX BOM, checksums, build-provenance attestations, and SBOM attestation pass. The workflow assembles the verified assets as a draft.
4. Review the draft release notes and assets, then approve the protected `release-approval` environment. Approval publishes that draft. Never re-run a release by replacing or editing its published assets.
5. Confirm the versioned Pages documentation is deployed after the release is published.

Each release includes five `tasc-v<version>-<target>.tar.gz` archives, the inspectable `tasc-command-skill-v<version>.tar.gz` package, `tasc.cdx.json`, and `SHA256SUMS.txt`. Build provenance is attached to each binary archive and the SBOM attestation carries the CycloneDX BOM predicate for the archives.

If assembly creates a draft but the workflow must be retried, delete only that unpublished draft before rerunning. Never delete or replace a published release.

## Verify a download

Download the selected archive and `SHA256SUMS.txt` from the same release and compare the archive's SHA-256 digest with its filename in the manifest. GitHub CLI can verify its build provenance:

```sh
gh attestation verify tasc-v0.1.0-x86_64-unknown-linux-gnu.tar.gz \
  --repo pascalvanderheiden/turbo-agent-skills-cli
```

The separate SBOM attestation for the archive can be verified with:

```sh
gh attestation verify tasc-v0.1.0-x86_64-unknown-linux-gnu.tar.gz \
  --repo pascalvanderheiden/turbo-agent-skills-cli \
  --predicate-type https://cyclonedx.org/bom
```

The standalone `tasc.cdx.json` is also published and covered by the release checksum manifest.

## Defective release correction

Never reuse a tag, replace an asset, or delete a published release. Warn users in the affected release notes, mark the affected release as non-latest/prerelease, and publish a corrected higher SemVer version with corrected notes. Keep the original assets and attestations available so existing downloads remain auditable. Document affected versions and manual replacement instructions on the release page.
