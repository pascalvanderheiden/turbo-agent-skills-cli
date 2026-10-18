import gzip
import hashlib
import json
import re
import tarfile
import tempfile
import unittest
from pathlib import Path

from release import (
    TARGETS,
    make_archive,
    make_command_skill_archive,
    prepare_cyclonedx_sbom,
    validate_assets,
    validate_tag,
    validate_version_metadata,
    write_checksums,
)


class ReleaseTests(unittest.TestCase):
    def test_release_workflow_matrix_covers_all_supported_targets(self):
        workflow = Path(__file__).resolve().parents[2] / ".github" / "workflows" / "release.yml"
        content = workflow.read_text()
        targets = set(re.findall(r"^\s+target: ([\w-]+)$", content, re.MULTILINE))
        self.assertEqual(targets, TARGETS)

    def test_publish_requires_and_verifies_immutable_release_configuration(self):
        workflow = Path(__file__).resolve().parents[2] / ".github" / "workflows" / "release.yml"
        content = workflow.read_text()
        guard = content.index("Require immutable-release configuration")
        publish = content.index("Publish immutable release")
        verify = content.index("Verify release immutability")

        self.assertIn("TASC_IMMUTABLE_RELEASES_ENABLED", content)
        self.assertIn("--jq .immutable", content)
        self.assertIn("if ! immutable=", content)
        self.assertIn("Could not verify or return", content)
        self.assertLess(guard, publish)
        self.assertLess(publish, verify)

    def test_tag_must_be_protected_stable_semver_matching_package(self):
        validate_tag("v0.1.0", "0.1.0", True)
        for tag, version, protected in (
            ("v0.1.0-rc.1", "0.1.0", True),
            ("v0.1.0", "0.2.0", True),
            ("v0.1.0", "0.1.0", False),
        ):
            with self.subTest(tag=tag, version=version, protected=protected):
                with self.assertRaises(ValueError):
                    validate_tag(tag, version, protected)

    def test_release_metadata_and_installer_versions_match(self):
        root = Path(__file__).resolve().parents[2]
        validate_version_metadata(root, "0.1.0")
        with self.assertRaisesRegex(ValueError, "does not match release"):
            validate_version_metadata(root, "0.2.0")

    def test_archives_reject_invalid_versions_and_targets(self):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "tasc"
            source.write_bytes(b"binary")
            with self.assertRaises(ValueError):
                make_archive(source, "x86_64-unknown-linux-gnu", "v0.1.0-rc.1", directory)
            with self.assertRaises(ValueError):
                make_archive(source, "x86_64-unknown-linux-musl", "v0.1.0", directory)

    def test_archive_metadata_and_contents_are_reproducible(self):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "tasc"
            source.write_bytes(b"binary")
            first = make_archive(source, "x86_64-unknown-linux-gnu", "v0.1.0", directory)
            first_bytes = first.read_bytes()
            second = make_archive(source, "x86_64-unknown-linux-gnu", "v0.1.0", directory)

            self.assertEqual(first_bytes, second.read_bytes())
            with gzip.open(first, "rb") as archive_bytes:
                import tarfile

                archive = tarfile.open(fileobj=archive_bytes, mode="r:")
                member = archive.getmember("tasc")
                self.assertEqual(member.mtime, 0)
                self.assertEqual(member.uid, 0)
                self.assertEqual(archive.extractfile(member).read(), b"binary")

    def test_command_skill_archive_is_reproducible_and_matches_release_version(self):
        skill_directory = Path(__file__).resolve().parents[2] / "skills" / "tasc-command"
        with tempfile.TemporaryDirectory() as directory:
            first = make_command_skill_archive(skill_directory, "v0.1.0", directory)
            first_bytes = first.read_bytes()
            second = make_command_skill_archive(skill_directory, "v0.1.0", directory)

            self.assertEqual(first_bytes, second.read_bytes())
            with tarfile.open(first, "r:gz") as archive:
                self.assertEqual(
                   set(archive.getnames()),
                   {
                       "tasc-command/SKILL.md",
                       "tasc-command/manifest.json",
                       "tasc-command/adapters/copilot.json",
                   },
                )
            with self.assertRaises(ValueError):
                make_command_skill_archive(skill_directory, "v0.2.0", directory)

    def test_asset_validation_requires_all_targets_sbom_and_matching_checksums(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            targets = (
                "x86_64-unknown-linux-gnu",
                "aarch64-unknown-linux-gnu",
                "x86_64-apple-darwin",
                "aarch64-apple-darwin",
                "x86_64-pc-windows-msvc",
            )
            for target in targets:
                (root / f"tasc-v0.1.0-{target}.tar.gz").write_bytes(target.encode())
            (root / "tasc.cdx.json").write_text(
                json.dumps(
                    {
                        "bomFormat": "CycloneDX",
                        "serialNumber": "urn:uuid:323cd151-d10f-5a98-96e7-50b88ed2e860",
                        "specVersion": "1.5",
                        "components": [{"name": "tasc"}],
                    }
                ),
                encoding="utf-8",
            )
            make_command_skill_archive(
                Path(__file__).resolve().parents[2] / "skills" / "tasc-command",
                "v0.1.0",
                root,
            )
            write_checksums(root)
            validate_assets(root, "v0.1.0")
            (root / "tasc.cdx.json").write_text("{}", encoding="utf-8")
            with self.assertRaises(ValueError):
                validate_assets(root, "v0.1.0")

    def test_reproducible_sbom_gets_deterministic_attestation_identity(self):
        with tempfile.TemporaryDirectory() as directory:
            sbom_path = Path(directory) / "tasc.cdx.json"
            sbom_path.write_text(
                json.dumps(
                    {
                        "bomFormat": "CycloneDX",
                        "specVersion": "1.5",
                        "version": 1,
                        "components": [{"name": "tasc"}],
                    }
                ),
                encoding="utf-8",
            )

            prepare_cyclonedx_sbom(sbom_path, "pascalvanderheiden/turbo-agent-skills-cli@v0.1.0")
            first = sbom_path.read_bytes()
            prepared = json.loads(first)

            self.assertRegex(
                prepared["serialNumber"],
                r"^urn:uuid:[0-9a-f]{8}-[0-9a-f]{4}-5[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$",
            )
            prepare_cyclonedx_sbom(sbom_path, "pascalvanderheiden/turbo-agent-skills-cli@v0.1.0")
            self.assertEqual(first, sbom_path.read_bytes())

    def test_asset_validation_rejects_missing_platform_and_tampered_checksums(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for target in (
                "x86_64-unknown-linux-gnu",
                "aarch64-unknown-linux-gnu",
                "x86_64-apple-darwin",
                "aarch64-apple-darwin",
                "x86_64-pc-windows-msvc",
            ):
                (root / f"tasc-v0.1.0-{target}.tar.gz").write_bytes(target.encode())
            (root / "tasc.cdx.json").write_text(
                json.dumps(
                    {
                        "bomFormat": "CycloneDX",
                        "serialNumber": "urn:uuid:323cd151-d10f-5a98-96e7-50b88ed2e860",
                        "specVersion": "1.5",
                        "components": [{"name": "tasc"}],
                    }
                ),
                encoding="utf-8",
            )
            make_command_skill_archive(
                Path(__file__).resolve().parents[2] / "skills" / "tasc-command",
                "v0.1.0",
                root,
            )
            write_checksums(root)
            (root / "tasc-v0.1.0-x86_64-pc-windows-msvc.tar.gz").unlink()
            with self.assertRaises(ValueError):
                validate_assets(root, "v0.1.0")

            archive = root / "tasc-v0.1.0-x86_64-pc-windows-msvc.tar.gz"
            archive.write_bytes(b"tampered")
            with self.assertRaises(ValueError):
                validate_assets(root, "v0.1.0")

    def test_checksum_file_covers_each_release_asset(self):
        with tempfile.TemporaryDirectory() as directory:
            asset = Path(directory) / "archive.tar.gz"
            asset.write_bytes(b"contents")
            checksums = write_checksums(directory)
            expected = f"{hashlib.sha256(b'contents').hexdigest()}  archive.tar.gz\n"
            self.assertEqual(checksums.read_text(encoding="utf-8"), expected)


if __name__ == "__main__":
    unittest.main()
