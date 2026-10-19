import hashlib
import os
import subprocess
import tarfile
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
INSTALLER = ROOT / "install.sh"


class InstallerTests(unittest.TestCase):
    def detect_target(self, operating_system, architecture):
        result = subprocess.run(
            [
                "bash",
                "-c",
                'uname() { if [ "$1" = "-s" ]; then printf "%s\\n" "$TEST_OS"; '
                'else printf "%s\\n" "$TEST_ARCH"; fi; }; '
                'source "$1"; detect_target',
                "_",
                str(INSTALLER),
            ],
            capture_output=True,
            text=True,
            env={
                **os.environ,
                "TEST_OS": operating_system,
                "TEST_ARCH": architecture,
            },
            check=False,
        )
        return result

    def test_detects_each_supported_platform(self):
        for operating_system, architecture, target in (
            ("Linux", "x86_64", "x86_64-unknown-linux-gnu"),
            ("Linux", "aarch64", "aarch64-unknown-linux-gnu"),
            ("Darwin", "x86_64", "x86_64-apple-darwin"),
            ("Darwin", "arm64", "aarch64-apple-darwin"),
            ("MINGW64_NT-10.0", "x86_64", "x86_64-pc-windows-msvc"),
        ):
            with self.subTest(target=target):
                result = self.detect_target(operating_system, architecture)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(result.stdout.strip(), target)

    def test_unsupported_platform_has_clear_error(self):
        result = self.detect_target("FreeBSD", "riscv64")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Unsupported operating system or CPU", result.stderr)

    def test_download_is_verified_before_install(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            assets = root / "assets"
            assets.mkdir()
            archive_path = assets / "tasc-v0.1.0-x86_64-unknown-linux-gnu.tar.gz"
            binary = root / "tasc"
            binary.write_text("#!/bin/sh\nprintf 'installed tasc\\n'\n", encoding="utf-8")
            with tarfile.open(archive_path, "w:gz") as archive:
                archive.add(binary, arcname="tasc")
            digest = hashlib.sha256(archive_path.read_bytes()).hexdigest()
            (assets / "SHA256SUMS.txt").write_text(
                f"{digest}  {archive_path.name}\n", encoding="utf-8"
            )

            bin_directory = root / "bin"
            bin_directory.mkdir()
            uname = bin_directory / "uname"
            uname.write_text(
                "#!/bin/sh\n"
                'if [ "$1" = "-s" ]; then printf "Linux\\n"; '
                'else printf "x86_64\\n"; fi\n',
                encoding="utf-8",
            )
            uname.chmod(0o755)
            curl = bin_directory / "curl"
            curl.write_text(
                "#!/bin/sh\n"
                'while [ "$#" -gt 0 ]; do\n'
                '  if [ "$1" = "--output" ]; then output="$2"; shift 2; else url="$1"; shift; fi\n'
                "done\n"
                'cp "$TEST_ASSETS/${url##*/}" "$output"\n',
                encoding="utf-8",
            )
            curl.chmod(0o755)
            install_dir = root / "installed"
            environment = {
                **os.environ,
                "PATH": f"{bin_directory}:{os.environ['PATH']}",
                "TEST_ASSETS": str(assets),
                "TASC_INSTALL_DIR": str(install_dir),
            }
            result = subprocess.run(
                ["bash", str(INSTALLER)],
                capture_output=True,
                text=True,
                env=environment,
                check=False,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(
                result.stderr.splitlines(),
                [
                    "DETECTING: platform",
                    "DOWNLOADING: tasc-v0.1.0-x86_64-unknown-linux-gnu.tar.gz",
                    "VERIFYING: checksum and archive",
                    f"INSTALLING: {install_dir}",
                ],
            )
            self.assertEqual(
                result.stdout.splitlines(),
                [
                    f"INSTALLED: TASC v0.1.0 to {install_dir}/tasc",
                    f"NEXT: Add {install_dir} to your PATH to run tasc.",
                ],
            )
            installed = install_dir / "tasc"
            self.assertTrue(installed.is_file())
            self.assertTrue(os.access(installed, os.X_OK))
            self.assertIn("installed tasc", subprocess.check_output([installed], text=True))

            (assets / "SHA256SUMS.txt").write_text(
                f"{'0' * 64}  {archive_path.name}\n", encoding="utf-8"
            )
            install_dir2 = root / "tampered-install"
            result = subprocess.run(
                ["bash", str(INSTALLER)],
                capture_output=True,
                text=True,
                env={**environment, "TASC_INSTALL_DIR": str(install_dir2)},
                check=False,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(
                result.stderr.splitlines(),
                [
                    "DETECTING: platform",
                    "DOWNLOADING: tasc-v0.1.0-x86_64-unknown-linux-gnu.tar.gz",
                    "VERIFYING: checksum and archive",
                    "ERROR: SHA-256 verification failed for "
                    "tasc-v0.1.0-x86_64-unknown-linux-gnu.tar.gz.",
                ],
            )
            self.assertEqual(result.stdout, "")
            self.assertFalse(install_dir2.exists())


if __name__ == "__main__":
    unittest.main()
