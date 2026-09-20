#!/usr/bin/env python3
"""Hardware-free checks for isolation, failure propagation and process cleanup."""

from contextlib import ExitStack
import importlib.util
import json
import os
import socket
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import Mock, patch

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("gui_session", Path(__file__).with_name("run-gui-tests.py"))
gui = importlib.util.module_from_spec(spec)
spec.loader.exec_module(gui)
PROCESS_TIMEOUT_SECONDS = 3
PROCESS_POLL_SECONDS = 0.01
CHILD_KEEPALIVE_SECONDS = 60
PROBE_FAILURE_TIMEOUT_SECONDS = 0.5
TEST_SINK = "nagametv_test_fixture"
TEST_MODULE_ID = 42
TEST_MONITOR_INDEX = 51
TEST_MONITOR_NAME = f"{TEST_SINK}.monitor"
TEST_RECORDER_PID = 12000


class PulseFixture:
    """Protocol replies only: these tests never connect to an audio server."""

    def __init__(self):
        self.modules = [{"index": 10, "name": "module-null-sink", "argument": "sink_name=other"}]
        self.sinks = [{"name": "speakers", "index": 11}]
        self.calls = []
        self.environments = []
        self.server_name = "PulseAudio (on PipeWire test)"
        self.load_error = None

    def probe(self, _client, *command):
        self.calls.append(command)
        self.environments.append(dict(_client.env))
        if command == ("pactl", "--format=json", "info"):
            return json.dumps({"server_name": self.server_name})
        if command[:2] == ("pactl", "load-module"):
            self.modules.append({"index": TEST_MODULE_ID, "name": command[2], "argument": " ".join(command[3:])})
            self.sinks.append({"name": TEST_SINK, "owner_module": TEST_MODULE_ID,
                               "monitor_source": TEST_MONITOR_NAME,
                               "properties": {"factory.name": "support.null-audio-sink"}})
            if self.load_error:
                raise self.load_error
            return str(TEST_MODULE_ID)
        if command == ("pactl", "list", "short", "modules"):
            return "".join(f'{item["index"]}\t{item["name"]}\t{item["argument"]}\t\n'
                           for item in self.modules)
        if command == ("pactl", "--format=json", "list", "sinks"):
            return json.dumps(self.sinks)
        if command == ("pactl", "unload-module", str(TEST_MODULE_ID)):
            self.modules = [item for item in self.modules if item["index"] != TEST_MODULE_ID]
            self.sinks = [item for item in self.sinks if item["name"] != TEST_SINK]
            return ""
        raise AssertionError(f"Unexpected audio operation: {command}")


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
    def setUp(self):
        self.runtime = Path(self.enterContext(tempfile.TemporaryDirectory()))
        endpoint = self.enterContext(socket.socket(socket.AF_UNIX))
        self.socket_path = self.runtime / "native"
        endpoint.bind(str(self.socket_path))
        self.server = gui.DetectedAudioServer.detect({"PULSE_SERVER": f"unix:{self.socket_path}"})

    def test_host_routes_and_stale_session_are_removed(self):
        inherited = dict(DISPLAY=":99", WAYLAND_DISPLAY="host", WAYLAND_SOCKET="42",
                         XAUTHORITY="host-auth", PULSE_SERVER="host-pulse", PULSE_COOKIE="host-cookie",
                         PIPEWIRE_REMOTE="host-pipewire", DBUS_SESSION_BUS_ADDRESS="host-bus",
                         DBUS_STARTER_ADDRESS="host-starter", NAGAMETV_GUI_SESSION_DIR="stale",
                         QT_QUICK_BACKEND="software", QT_QPA_PLATFORM="offscreen",
                         CARGO_TARGET_DIR="build/cargo")
        env = gui.isolated_environment(Path("/tmp/test-session"), inherited, self.server, TEST_SINK)
        for name in ("DISPLAY", "WAYLAND_DISPLAY", "WAYLAND_SOCKET", "XAUTHORITY",
                     "PIPEWIRE_REMOTE", "PULSE_COOKIE", "DBUS_STARTER_ADDRESS",
                     "NAGAMETV_GUI_SESSION_DIR", "QT_QUICK_BACKEND"):
            self.assertNotIn(name, env)
        self.assertEqual(env["PULSE_SERVER"], self.server.address)
        self.assertEqual(env["PULSE_SINK"], TEST_SINK)
        self.assertEqual(env["PULSE_SOURCE"], f"{TEST_SINK}.monitor")
        self.assertIn("node.dont-fallback=true", env["PULSE_PROP"])
        self.assertEqual(env["DBUS_SESSION_BUS_ADDRESS"], "unix:path=/tmp/test-session/bus")
        self.assertEqual(env["CARGO_TARGET_DIR"], "build/cargo")
        self.assertEqual(env["NAGAMETV_AUDIO_SINK"], "pulsesink")
        self.assertEqual(inherited["DISPLAY"], ":99")

    def test_endpoint_is_resolved_before_replacing_runtime_directory(self):
        (self.runtime / "pulse").mkdir()
        endpoint = self.enterContext(socket.socket(socket.AF_UNIX))
        endpoint.bind(str(self.runtime / "pulse/native"))
        detected = gui.DetectedAudioServer.detect({"XDG_RUNTIME_DIR": str(self.runtime)})
        self.assertEqual(detected.address, f"unix:{self.runtime}/pulse/native")
        selected = gui.DetectedAudioServer.detect({"XDG_RUNTIME_DIR": str(self.runtime),
                                                  "PULSE_SERVER": self.server.address})
        self.assertEqual(selected.address, self.server.address)

    def test_missing_invalid_and_replaced_endpoints_are_rejected(self):
        for address in ("", "tcp:localhost:4713", "unix:relative", "unix:/a unix:/b",
                        f"unix:{self.runtime}/missing"):
            with self.subTest(address=address), self.assertRaises(RuntimeError):
                gui.DetectedAudioServer.detect({"PULSE_SERVER": address})
        replacement = self.enterContext(socket.socket(socket.AF_UNIX))
        replacement.bind(str(self.runtime / "replacement"))
        (self.runtime / "replacement").replace(self.socket_path)
        with self.assertRaisesRegex(RuntimeError, "replaced"):
            self.server.check()

    def audio_fixture(self):
        backend = PulseFixture()
        self.enterContext(patch.object(gui._Processes, "probe", autospec=True, side_effect=backend.probe))
        self.enterContext(patch.object(gui, "validate_audio"))
        env = gui.isolated_environment(self.runtime, {}, self.server, TEST_SINK)
        return backend, env

    def test_shared_server_keeps_other_outputs_and_only_releases_owned_module(self):
        backend, env = self.audio_fixture()
        with gui.ValidatedAudio.open(self.server, env, self.runtime, self.runtime) as audio:
            audio.check()
            self.assertEqual(len(backend.sinks), 2)
        self.assertEqual(backend.sinks, [{"name": "speakers", "index": 11}])
        self.assertEqual([item["index"] for item in backend.modules], [10])
        with self.assertRaisesRegex(RuntimeError, "closed"):
            audio.check()
        with self.assertRaises(TypeError):
            gui.ValidatedAudio()

    def test_audio_lease_cannot_use_an_unchecked_or_changed_endpoint(self):
        backend, env = self.audio_fixture()
        with self.assertRaisesRegex(RuntimeError, "does not match"):
            with gui.ValidatedAudio.open(self.server, {**env, "PULSE_SERVER": "unix:/other"},
                                         self.runtime, self.runtime):
                self.fail("Unchecked audio endpoint escaped")
        self.assertEqual(backend.calls, [])
        with gui.ValidatedAudio.open(self.server, env, self.runtime, self.runtime):
            env["PULSE_SERVER"] = "unix:/other"
        self.assertTrue(all(item["PULSE_SERVER"] == self.server.address for item in backend.environments))
        self.assertEqual([item["index"] for item in backend.modules], [10])

    def test_loopback_failure_releases_output_without_publishing_validation(self):
        backend, env = self.audio_fixture()
        with patch.object(gui, "validate_audio", side_effect=RuntimeError("no audio samples")):
            with self.assertRaisesRegex(RuntimeError, "no audio samples"):
                with gui.ValidatedAudio.open(self.server, env, self.runtime, self.runtime):
                    self.fail("Failed audio validation escaped")
        self.assertEqual([item["index"] for item in backend.modules], [10])

    def test_lost_load_reply_still_releases_owned_module(self):
        backend, env = self.audio_fixture()
        backend.load_error = subprocess.TimeoutExpired("pactl", 1)
        with self.assertRaises(subprocess.TimeoutExpired):
            with gui.ValidatedAudio.open(self.server, env, self.runtime, self.runtime):
                self.fail("Timed-out load escaped")
        self.assertEqual([item["index"] for item in backend.modules], [10])

    def test_interruption_releases_owned_output(self):
        backend, env = self.audio_fixture()
        with self.assertRaises(KeyboardInterrupt):
            with gui.ValidatedAudio.open(self.server, env, self.runtime, self.runtime):
                raise KeyboardInterrupt
        self.assertEqual([item["index"] for item in backend.modules], [10])

    def test_server_replacement_aborts_cleanup_before_any_unload(self):
        backend, env = self.audio_fixture()
        with self.assertRaisesRegex(RuntimeError, "replaced"):
            with gui.ValidatedAudio.open(self.server, env, self.runtime, self.runtime):
                replacement = self.enterContext(socket.socket(socket.AF_UNIX))
                replacement.bind(str(self.runtime / "replacement"))
                (self.runtime / "replacement").replace(self.socket_path)
        self.assertFalse(any(command[1] == "unload-module" for command in backend.calls))

    def test_monitor_wait_ignores_other_recorders_and_other_sources(self):
        client = Mock()
        sources = [{"index": TEST_MONITOR_INDEX, "name": TEST_MONITOR_NAME},
                   {"index": TEST_MONITOR_INDEX + 1, "name": "microphone"}]
        outputs = [{"source": TEST_MONITOR_INDEX,
                    "properties": {"application.process.id": str(TEST_RECORDER_PID + 1)}},
                   {"source": TEST_MONITOR_INDEX + 1,
                    "properties": {"application.process.id": str(TEST_RECORDER_PID)}}]
        client.probe.side_effect = lambda *args: json.dumps(sources if args[-1] == "sources" else outputs)
        self.assertFalse(gui.monitor_connected(client, TEST_MONITOR_NAME, TEST_RECORDER_PID))
        outputs.append({"source": TEST_MONITOR_INDEX,
                        "properties": {"application.process.id": str(TEST_RECORDER_PID)}})
        self.assertTrue(gui.monitor_connected(client, TEST_MONITOR_NAME, TEST_RECORDER_PID))
        sources.pop(0)
        self.assertFalse(gui.monitor_connected(client, TEST_MONITOR_NAME, TEST_RECORDER_PID))

    def test_module_ids_survive_multiline_unrelated_module_arguments(self):
        client = Mock()
        client.probe.return_value = (
            '1\tlibpipewire-module-rt\t{\n  nice.level = -11\n}\t\n'
            f'{TEST_MODULE_ID}\tmodule-null-sink\tsink_name={TEST_SINK} rate=48000\t\n'
            f'{TEST_MODULE_ID + 1}\tmodule-null-sink\tsink_name={TEST_SINK}_other\t\n')
        self.assertEqual(gui.owned_audio_modules(client, TEST_SINK), [TEST_MODULE_ID])

    def test_nested_gui_script_rejects_changed_audio_routes(self):
        env = gui.isolated_environment(self.runtime, dict(os.environ), self.server, TEST_SINK)
        env.update(NAGAMETV_GUI_SESSION_DIR=str(self.runtime), DISPLAY=":99", WAYLAND_DISPLAY="nagametv-wayland")
        (self.runtime / "display").write_text(env["DISPLAY"])
        (self.runtime / "pulse-server").write_text(env["PULSE_SERVER"])
        (self.runtime / "pulse-sink").write_text(env["PULSE_SINK"])
        command = ["bash", "-euc", "source scripts/gui-test-session.sh"]
        subprocess.run(command, cwd=gui.ROOT, env=env, check=True)
        for changed in ("PULSE_SERVER", "PULSE_SINK", "PULSE_SOURCE"):
            with self.subTest(changed=changed):
                result = subprocess.run(command, cwd=gui.ROOT, env={**env, changed: "changed"},
                                        text=True, capture_output=True)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("Invalid or closed", result.stderr)

    def test_pulseaudio_daemon_is_rejected_before_mutation(self):
        backend, env = self.audio_fixture()
        backend.server_name = "pulseaudio"
        with self.assertRaisesRegex(RuntimeError, "PipeWire"):
            with gui.ValidatedAudio.open(self.server, env, self.runtime, self.runtime):
                self.fail("Wrong server escaped")
        self.assertEqual(backend.calls, [("pactl", "--format=json", "info")])

    def test_module_id_reuse_does_not_unload_another_sessions_output(self):
        backend, env = self.audio_fixture()
        with gui.ValidatedAudio.open(self.server, env, self.runtime, self.runtime):
            backend.modules[-1]["argument"] = "sink_name=another_session"
        self.assertEqual(len(backend.modules), 2)
        self.assertFalse(any(command[1] == "unload-module" for command in backend.calls))

    def test_disappearing_output_stops_command_and_cleans_up_process(self):
        backend, env = self.audio_fixture()
        pid_file = self.runtime / "command.pid"
        with gui.ValidatedAudio.open(self.server, env, self.runtime, self.runtime) as audio, ExitStack() as stack:
            processes = gui._Processes(stack, dict(os.environ), self.runtime)

            def disappear_after_command_starts():
                if pid_file.exists():
                    backend.sinks = []
                audio.check()

            processes.validators.append(disappear_after_command_starts)
            with self.assertRaisesRegex(RuntimeError, "disappeared"):
                gui._run_command(processes, [sys.executable, "-c",
                    "import os,pathlib,sys,time; pathlib.Path(sys.argv[1]).write_text(str(os.getpid())); "
                    f"time.sleep({CHILD_KEEPALIVE_SECONDS})", str(pid_file)])
        self.assertTrue(stopped(int(pid_file.read_text())))

    def test_owned_name_cannot_substitute_a_physical_or_foreign_sink(self):
        backend, env = self.audio_fixture()
        with gui.ValidatedAudio.open(self.server, env, self.runtime, self.runtime) as audio:
            backend.sinks[-1]["properties"]["factory.name"] = "api.alsa.pcm.sink"
            with self.assertRaisesRegex(RuntimeError, "owned PipeWire null sink"):
                audio.check()

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
