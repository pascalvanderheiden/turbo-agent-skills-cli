import errno
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

if os.name == "posix":
    import pty


ROOT = Path(__file__).resolve().parents[2]
BINARY = ROOT / "target/debug/tasc"
ERROR_BODY = (
    b"offline: This command requires GitHub access. "
    b"Remove --offline to explicitly allow the operation."
)
JSON_ERROR = (
    b'{"schemaVersion":1,"command":"skill search","diagnostic":'
    b'{"code":"offline","category":"usage","origin":"tasc",'
    b'"message":"This command requires GitHub access.",'
    b'"remediation":"Remove --offline to explicitly allow the operation."}}\n'
)
EVENTS = (
    ("Input", "›", "36"),
    ("TASC", "◆", "35"),
    ("Prompt", "?", "36"),
    ("Info", "·", "34"),
    ("Working", "...", "36"),
    ("Warning", "!", "33"),
    ("Error", "×", "31"),
    ("Done", "✓", "32"),
    ("Next", "↳", "2;36"),
)


def environment(**overrides):
    result = {**os.environ, "TERM": "xterm-256color"}
    result.pop("NO_COLOR", None)
    result.update(overrides)
    return result


def terminal_output(command, env, stream="stderr"):
    master, slave = pty.openpty()
    try:
        with subprocess.Popen(
            command,
            stdin=subprocess.DEVNULL,
            stdout=slave if stream == "stdout" else subprocess.PIPE,
            stderr=slave if stream == "stderr" else subprocess.PIPE,
            env=env,
        ) as process:
            stdout, stderr = process.communicate(timeout=15)
            output = bytearray()
            os.close(slave)
            slave = None
            while True:
                try:
                    chunk = os.read(master, 4096)
                    if not chunk:
                        break
                    output.extend(chunk)
                except OSError as error:
                    if error.errno != errno.EIO:
                        raise
                    break
            rendered = bytes(output).replace(b"\r\n", b"\n")
            return process.returncode, stdout, stderr, rendered
    finally:
        if slave is not None:
            os.close(slave)
        os.close(master)


@unittest.skipUnless(BINARY.is_file(), "build tasc before running process tests")
class PresentationProcessTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.env = environment(
            TASC_STATE_PATH=str(Path(self.directory.name) / "state.json"),
            HOME=self.directory.name,
        )
        self.command = [str(BINARY), "--offline", "search", "review"]

    def test_piped_errors_and_json_contract_are_stable_under_all_gates(self):
        for gate in ({}, {"NO_COLOR": ""}, {"NO_COLOR": "1"}, {"TERM": "dumb"}):
            for json_args in ([], ["--json"], ["--format", "json"]):
                with self.subTest(gate=gate, json_args=json_args):
                    result = subprocess.run(
                        self.command + json_args,
                        env={**self.env, **gate},
                        capture_output=True,
                        check=False,
                        timeout=15,
                    )
                    self.assertEqual(result.returncode, 2)
                    self.assertEqual(result.stdout, b"")
                    self.assertEqual(
                        result.stderr,
                        JSON_ERROR if json_args else b"ERROR: " + ERROR_BODY + b"\n",
                    )

    @unittest.skipUnless(os.name == "posix", "requires a pseudo-terminal")
    def test_tty_error_decoration_and_gating_preserve_exit_and_json_bytes(self):
        for gate in ({}, {"NO_COLOR": ""}, {"NO_COLOR": "1"}, {"TERM": "dumb"}):
            for json_args in ([], ["--json"], ["--format", "json"]):
                with self.subTest(gate=gate, json_args=json_args):
                    code, stdout, _, stderr = terminal_output(
                        self.command + json_args, {**self.env, **gate}
                    )
                    self.assertEqual(code, 2)
                    self.assertEqual(stdout, b"")
                    expected = JSON_ERROR
                    if not json_args:
                        prefix = b"ERROR:" if gate else "\x1b[31m× Error\x1b[0m".encode()
                        expected = prefix + b" " + ERROR_BODY + b"\n"
                    self.assertEqual(stderr, expected)

    def test_success_results_stay_on_stdout(self):
        baseline = None
        for gate in ({}, {"NO_COLOR": ""}, {"TERM": "dumb"}):
            result = subprocess.run(
                [str(BINARY), "source", "capabilities", "--json"],
                env={**self.env, **gate},
                capture_output=True,
                check=False,
                timeout=15,
            )
            self.assertEqual(result.returncode, 0)
            self.assertEqual(result.stderr, b"")
            self.assertNotIn(b"\x1b", result.stdout)
            if baseline is None:
                baseline = result.stdout
            self.assertEqual(result.stdout, baseline)


@unittest.skipUnless(os.name == "posix", "requires Bash and pseudo-terminals")
class InstallerPresentationTests(unittest.TestCase):
    def test_bash_vocabulary_routing_and_capability_gates(self):
        for category, glyph, color in EVENTS:
            stream = "stderr" if category in ("Prompt", "Working", "Error", "Warning") else "stdout"
            command = [
                "bash", "-c", 'source "$1"; presentation_event "$2" body',
                "_", str(ROOT / "install.sh"), category,
            ]
            for gate in ({}, {"NO_COLOR": ""}, {"TERM": "dumb"}):
                with self.subTest(category=category, gate=gate):
                    env = environment(**gate)
                    code, stdout, stderr, rendered = terminal_output(command, env, stream)
                    self.assertEqual(code, 0)
                    self.assertEqual(stdout if stream == "stderr" else stderr, b"")
                    prefix = (
                        f"{category.upper()}:"
                        if gate else f"\x1b[{color}m{glyph} {category}\x1b[0m"
                    )
                    self.assertEqual(rendered, f"{prefix} body\n".encode())
                    piped = subprocess.run(command, env=env, capture_output=True, check=False)
                    self.assertEqual(piped.returncode, 0)
                    output = piped.stderr if stream == "stderr" else piped.stdout
                    self.assertEqual(output, f"{category.upper()}: body\n".encode())
                    self.assertEqual(piped.stdout if stream == "stderr" else piped.stderr, b"")

    @unittest.skipUnless(
        shutil.which("pwsh") and sys.platform == "linux",
        "requires PowerShell on an unsupported installer platform",
    )
    def test_powershell_installer_errors_are_plain_and_on_stderr(self):
        for gate in ({}, {"NO_COLOR": ""}, {"TERM": "dumb"}):
            result = subprocess.run(
                ["pwsh", "-NoProfile", "-File", str(ROOT / "install.ps1")],
                env=environment(**gate),
                capture_output=True,
                check=False,
                timeout=15,
            )
            self.assertEqual(result.returncode, 1)
            self.assertEqual(result.stdout, b"")
            self.assertEqual(result.stderr, b"ERROR: This installer supports Windows x64 only.\n")


if __name__ == "__main__":
    unittest.main()
