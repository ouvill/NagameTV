"""Exercise the manual snap updater with local fakes for external commands."""

from contextlib import redirect_stderr, redirect_stdout
import hashlib
import io
from pathlib import Path
import shlex
import subprocess
import unittest
from unittest.mock import patch

from scripts.dev import update_snap


TARGET = "tono@pi"
STAGING = "/tmp/nagametv-snap.A1b2C3d4E5"
ARCHIVE = "nagametv_0.9.1_arm64.snap"
ARTIFACT = b"CI snap"
DIGEST = hashlib.sha256(ARTIFACT).hexdigest()
GPU = ("nagametv:gpu-2604", "mesa-2604:gpu-2604")
CEC = ("nagametv:hdmi-cec", "pi-gadget:cec")
ALSA = ("nagametv:alsa", ":alsa")
WAYLAND = ("nagametv:wayland", ":wayland")
NETWORK = ("nagametv:network", ":network")


class FakeCommands:
    def __init__(self, failure=None):
        self.failure = failure
        self.calls = []
        self.remote = []
        self.installed = False
        self.uploaded = False
        self.restored = []

    def __call__(self, command, *, check, text, stdout):
        self.calls.append(command)
        if command == ["git", "branch", "--show-current"]:
            output = "work/snap\n"
        elif command[:3] == ["gh", "pr", "view"]:
            output = '{"headRefName":"work/snap","headRefOid":"revision"}'
        elif command[:3] == ["gh", "run", "list"]:
            output = "" if self.failure == "no_run" else "123\n"
        elif command[:3] == ["gh", "run", "download"]:
            if self.failure == "missing_artifact":
                raise subprocess.CalledProcessError(1, command)
            directory = Path(command[command.index("--dir") + 1])
            (directory / ARCHIVE).write_bytes(ARTIFACT)
            digest = "0" * 64 if self.failure == "checksum" else DIGEST
            if self.failure != "missing_checksum":
                (directory / f"{ARCHIVE}.sha256").write_text(f"{digest}  {ARCHIVE}\n")
            output = ""
        elif command[:2] == ["scp", "--"]:
            if self.failure == "scp":
                raise subprocess.CalledProcessError(1, command)
            if command[-1] != f"{TARGET}:{STAGING}/":
                raise AssertionError(command)
            self.uploaded = Path(command[2]).read_bytes() == ARTIFACT
            output = ""
        elif command[:3] == ["ssh", "--", TARGET]:
            remote = shlex.split(command[3])
            if remote[:2] != ["env", "LC_ALL=C"]:
                raise AssertionError(command)
            output = self.ssh(remote[2:], command)
        else:
            raise AssertionError(command)
        return subprocess.CompletedProcess(command, 0, stdout=output)

    def ssh(self, remote, invocation):
        self.remote.append(remote)
        match remote:
            case ["snap", "version"]:
                if self.failure == "ssh":
                    raise subprocess.CalledProcessError(255, invocation)
                if self.failure == "architecture":
                    return "snap 2.78\narchitecture riscv64\n"
                return "snap 2.78\narchitecture  arm64\n"
            case ["mktemp", "-d", "/tmp/nagametv-snap.XXXXXXXXXX"]:
                return f"{STAGING}\n"
            case ["sha256sum", "--", archive]:
                if not self.uploaded or archive != f"{STAGING}/{ARCHIVE}":
                    raise AssertionError(remote)
                digest = "0" * 64 if self.failure == "remote_checksum" else DIGEST
                return f"{digest}  {archive}\n"
            case ["snap", "list", "nagametv"]:
                revision = "x6" if self.installed else "x5"
                return f"Name Version Rev Tracking Publisher Notes\nnagametv 0.9.1 {revision} - - -\n"
            case ["snap", "connections", "nagametv"]:
                pairs = {WAYLAND: "manual"} if self.installed else {
                    GPU: "-", CEC: "manual", ALSA: "manual", WAYLAND: "manual", NETWORK: "-",
                }
                if self.failure != "unrestored":
                    pairs.update(dict.fromkeys(self.restored, "manual"))
                rows = [f"interface {plug} {slot} {notes}" for (plug, slot), notes in pairs.items()]
                return "\n".join(["Interface Plug Slot Notes", *rows, "home nagametv:home - -"])
            case ["sudo", "-n", "snap", "install", "--dangerous", archive]:
                if self.failure == "sudo":
                    raise subprocess.CalledProcessError(1, invocation)
                if archive != f"{STAGING}/{ARCHIVE}":
                    raise AssertionError(remote)
                self.installed = True
                return ""
            case ["sudo", "-n", "snap", "connect", plug, slot]:
                self.restored.append((plug, slot))
                return ""
            case ["rm", "-r", "--", "/tmp/nagametv-snap.A1b2C3d4E5"]:
                return ""
            case _:
                raise AssertionError(remote)


class UpdateSnapTests(unittest.TestCase):
    def invoke(self, *options, failure=None):
        commands = FakeCommands(failure)
        stdout, stderr = io.StringIO(), io.StringIO()
        code = 0
        with (
            patch.object(update_snap.subprocess, "run", side_effect=commands),
            patch("sys.argv", ["update_snap", TARGET, *options]),
            patch("sys.stdin", io.StringIO()),
            redirect_stdout(stdout),
            redirect_stderr(stderr),
        ):
            try:
                update_snap.main()
            except SystemExit as error:
                code = error.code
        return commands, code, stdout.getvalue(), stderr.getvalue()

    def test_default_resolves_current_branch_and_restores_missing_links(self):
        commands, code, stdout, stderr = self.invoke()
        self.assertEqual((code, stderr), (0, ""))
        self.assertEqual(commands.calls[1], ["git", "branch", "--show-current"])
        self.assertIn("--branch=work/snap", commands.calls[2])
        self.assertIn("snap.yml", commands.calls[2])
        self.assertIn("success", commands.calls[2])
        self.assertEqual(commands.calls[3][:6], ["gh", "run", "download", "123", "--name", "snap-arm64"])
        self.assertEqual(commands.restored, sorted([GPU, CEC, ALSA]))
        snapshots = [cmd for cmd in commands.remote if cmd == ["snap", "connections", "nagametv"]]
        self.assertEqual(len(snapshots), 3)
        self.assertEqual(commands.remote[-1], ["rm", "-r", "--", STAGING])
        self.assertIn("revision x6", stdout)
        self.assertIn("sudo snap revert --revision=x5 nagametv", stdout)

    def test_source_options_select_the_requested_run(self):
        commands, code, _, stderr = self.invoke("--pr", "23")
        self.assertEqual((code, stderr), (0, ""))
        self.assertEqual(commands.calls[1][:4], ["gh", "pr", "view", "23"])
        self.assertEqual(commands.calls[2][-4:], ["--commit", "revision", "--event", "pull_request"])

        commands, code, _, stderr = self.invoke("--branch", "topic")
        self.assertEqual((code, stderr), (0, ""))
        self.assertEqual(commands.calls[1][:3], ["gh", "run", "list"])
        self.assertIn("--branch=topic", commands.calls[1])

        commands, code, _, stderr = self.invoke("--run", "999")
        self.assertEqual((code, stderr), (0, ""))
        self.assertEqual(commands.calls[1][:4], ["gh", "run", "download", "999"])

    def test_missing_source_or_bad_checksum_never_stages_on_device(self):
        for failure, message in [
            ("no_run", "No successful Snap packages run"),
            ("architecture", "Unsupported snap architecture"),
            ("missing_artifact", "gh', 'run', 'download'"),
            ("missing_checksum", "Missing checksum"),
            ("checksum", "SHA-256 mismatch"),
        ]:
            with self.subTest(failure=failure):
                commands, code, _, stderr = self.invoke(failure=failure)
                self.assertEqual(code, 1)
                self.assertIn(message, stderr)
                self.assertEqual(commands.remote, [["snap", "version"]])

    def test_ssh_sudo_and_transfer_errors_stop_the_update(self):
        for failure, message in [
            ("ssh", "exit status 255"),
            ("scp", "Command '['scp'"),
            ("sudo", "sudo -n snap install"),
            ("remote_checksum", "SHA-256 mismatch for the uploaded snap"),
        ]:
            with self.subTest(failure=failure):
                commands, code, _, stderr = self.invoke("--run", "999", failure=failure)
                self.assertEqual(code, 1)
                self.assertIn(message, stderr)
                self.assertFalse(commands.installed)
                self.assertEqual(commands.restored, [])
                if failure != "ssh":
                    self.assertEqual(commands.remote[-1], ["rm", "-r", "--", STAGING])

    def test_connection_check_fails_when_connect_did_not_restore_links(self):
        commands, code, stdout, stderr = self.invoke("--run", "999", failure="unrestored")
        self.assertEqual(code, 1)
        self.assertIn("Connections still missing", stderr)
        self.assertIn("To revert", stdout)
        self.assertEqual(commands.remote[-1], ["rm", "-r", "--", STAGING])
