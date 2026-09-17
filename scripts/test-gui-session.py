#!/usr/bin/env python3
"""Hardware-free checks for isolation, failure propagation and process cleanup."""

from contextlib import ExitStack
import importlib.util
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("gui_session", Path(__file__).with_name("run-gui-tests.py"))
gui = importlib.util.module_from_spec(spec)
spec.loader.exec_module(gui)
PROCESS_TIMEOUT_SECONDS = 3
PROCESS_POLL_SECONDS = 0.01
CHILD_KEEPALIVE_SECONDS = 60
PROBE_FAILURE_TIMEOUT_SECONDS = 0.5


def wait_for(predicate):
    deadline = time.monotonic() + PROCESS_TIMEOUT_SECONDS
    while not predicate():
        if time.monotonic() >= deadline:
            raise AssertionError("Child process did not reach the expected state")
        time.sleep(PROCESS_POLL_SECONDS)


def stopped(pid):
    status = Path(f"/proc/{pid}/stat")
    return not status.exists() or status.read_text().split()[2] == "Z"


class GuiSessionTests(unittest.TestCase):
    def test_host_routes_and_stale_session_are_removed(self):
        inherited = dict(DISPLAY=":99", WAYLAND_DISPLAY="host", WAYLAND_SOCKET="42",
                         XAUTHORITY="host-auth", PULSE_SERVER="host-pulse", PULSE_COOKIE="host-cookie",
                         PIPEWIRE_REMOTE="host-pipewire", DBUS_SESSION_BUS_ADDRESS="host-bus",
                         DBUS_STARTER_ADDRESS="host-starter", MIRAKURUN_GUI_SESSION_DIR="stale",
                         QT_QUICK_BACKEND="software", QT_QPA_PLATFORM="offscreen",
                         CARGO_TARGET_DIR="build/cargo")
        env = gui.isolated_environment(Path("/tmp/test-session"), inherited)
        for name in ("DISPLAY", "WAYLAND_DISPLAY", "WAYLAND_SOCKET", "XAUTHORITY",
                     "PIPEWIRE_REMOTE", "DBUS_STARTER_ADDRESS", "MIRAKURUN_GUI_SESSION_DIR", "QT_QUICK_BACKEND"):
            self.assertNotIn(name, env)
        self.assertEqual(env["PULSE_SERVER"], "unix:/tmp/test-session/pulse.sock")
        self.assertEqual(env["PULSE_COOKIE"], "/tmp/test-session/pulse.cookie")
        self.assertEqual(env["DBUS_SESSION_BUS_ADDRESS"], "unix:path=/tmp/test-session/bus")
        self.assertEqual(env["CARGO_TARGET_DIR"], "build/cargo")
        self.assertEqual(env["MIRAKURUN_AUDIO_SINK"], "pulsesink")
        self.assertEqual(inherited["DISPLAY"], ":99")

    def test_software_and_unverified_renderers_are_rejected(self):
        for renderer in ("llvmpipe", "softpipe", "Software Rasterizer", "swrast"):
            with self.subTest(renderer=renderer), self.assertRaises(RuntimeError):
                gui.validate_renderer(f"direct rendering: Yes\nOpenGL renderer string: {renderer}")
        for info in ("", "direct rendering: Yes", "direct rendering: No\nOpenGL renderer string: GPU",
                     "direct rendering: Yes\nOpenGL renderer string: GPU\nAccelerated: no"):
            with self.assertRaises(RuntimeError):
                gui.validate_renderer(info)
        gui.validate_renderer("direct rendering: Yes\nOpenGL renderer string: NVIDIA GPU")
        gui.validate_renderer("direct rendering: Yes\nOpenGL renderer string: AMD GPU\nAccelerated: yes")

    def test_missing_resources_cannot_publish_or_start_a_session(self):
        with patch.object(gui, "detect_resources", side_effect=RuntimeError("missing GPU")), \
             patch.object(gui.subprocess, "Popen") as start:
            with self.assertRaisesRegex(RuntimeError, "missing GPU"):
                with gui.ValidatedSession.open(Path("/unused")):
                    self.fail("unvalidated session escaped")
            start.assert_not_called()
        with self.assertRaises(TypeError):
            gui.ValidatedSession()

    def test_exit_status_and_argument_boundaries(self):
        with tempfile.TemporaryDirectory() as directory, ExitStack() as stack:
            processes = gui._Processes(stack, dict(os.environ), Path(directory))
            result = gui._run_command(processes, [sys.executable, "-c",
                "import sys; assert sys.argv[1:] == ['a b', '録画', '', '$(false)']; sys.exit(23)",
                "a b", "録画", "", "$(false)"])
            self.assertEqual(result, 23)

    def test_failure_of_a_service_stops_the_running_command(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            pid_file = root / "command.pid"
            with ExitStack() as stack:
                processes = gui._Processes(stack, dict(os.environ), root)
                processes.start("service", [sys.executable, "-c",
                    "import pathlib,sys,time; p=pathlib.Path(sys.argv[1]); "
                    f"exec('while not p.exists(): time.sleep({PROCESS_POLL_SECONDS})'); sys.exit(7)", str(pid_file)])
                with self.assertRaisesRegex(RuntimeError, "service exited with 7"):
                    gui._run_command(processes, [sys.executable, "-c",
                        "import os,pathlib,sys,time; pathlib.Path(sys.argv[1]).write_text(str(os.getpid())); "
                        f"time.sleep({CHILD_KEEPALIVE_SECONDS})",
                        str(pid_file)])
            self.assertTrue(stopped(int(pid_file.read_text())))

    def test_cleanup_kills_descendants_after_group_leader_exits(self):
        with tempfile.TemporaryDirectory() as directory:
            pid_file = Path(directory) / "descendant.pid"
            child = ("import os,pathlib,signal,sys,time; signal.signal(signal.SIGTERM, signal.SIG_IGN); "
                     "pathlib.Path(sys.argv[1]).write_text(str(os.getpid())); "
                     f"time.sleep({CHILD_KEEPALIVE_SECONDS})")
            leader = subprocess.Popen([sys.executable, "-c",
                "import subprocess,sys; subprocess.Popen([sys.executable, '-c', sys.argv[1], sys.argv[2]])",
                child, str(pid_file)], start_new_session=True)
            try:
                leader.wait(timeout=PROCESS_TIMEOUT_SECONDS)
                wait_for(pid_file.exists)
            finally:
                gui.stop_process_group(leader)
            wait_for(lambda: stopped(int(pid_file.read_text())))

    def test_wait_timeout_cleans_up_started_services(self):
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaisesRegex(RuntimeError, "Timed out"), ExitStack() as stack:
                processes = gui._Processes(stack, dict(os.environ), Path(directory))
                child = processes.start("service", [sys.executable, "-c",
                    f"import time; time.sleep({CHILD_KEEPALIVE_SECONDS})"])
                with patch.object(gui, "START_TIMEOUT_SECONDS", gui.POLL_SECONDS):
                    processes.until("missing socket", lambda: False)
            self.assertIsNotNone(child.poll())

    def test_probe_timeout_stops_the_probe_process_group(self):
        with tempfile.TemporaryDirectory() as directory, ExitStack() as stack:
            root = Path(directory)
            pid_file = root / "probe.pid"
            processes = gui._Processes(stack, dict(os.environ), root)
            with patch.object(gui, "PROBE_TIMEOUT_SECONDS", PROBE_FAILURE_TIMEOUT_SECONDS):
                with self.assertRaises(subprocess.TimeoutExpired):
                    processes.probe(sys.executable, "-c",
                        "import os,pathlib,sys,time; pathlib.Path(sys.argv[1]).write_text(str(os.getpid())); "
                        f"time.sleep({CHILD_KEEPALIVE_SECONDS})", str(pid_file))
            self.assertTrue(stopped(int(pid_file.read_text())))


if __name__ == "__main__":
    unittest.main()
