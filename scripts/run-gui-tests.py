#!/usr/bin/env python3
"""Linux: run GUI tests on a private GPU display and clocked virtual audio sink."""

import argparse
from array import array
from contextlib import contextmanager, ExitStack
import json
import os
from pathlib import Path
import re
import shlex
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import uuid

ROOT = Path(__file__).resolve().parent.parent
START_TIMEOUT_SECONDS = 20
PROBE_TIMEOUT_SECONDS = 15
STOP_TIMEOUT_SECONDS = 3
POLL_SECONDS = 0.05
SIGNAL_EXIT_BASE = 128
OUTPUT_WIDTH = 1920
OUTPUT_HEIGHT = 1080
AUDIO_RATE = 48000
AUDIO_CHANNELS = 2
AUDIO_SINK_PREFIX = "nagametv_test_"
AUDIO_CHECK_INTERVAL_SECONDS = 1
AUDIO_MONITOR_LATENCY_MS = 20
AUDIO_PROBE_BUFFERS = 10
MIN_AUDIO_PROBE_PEAK = 1000
SOFTWARE_RENDERERS = ("llvmpipe", "softpipe", "software rasterizer", "swrast")


class DetectedAudioServer:
    """A detected local socket, before any audio connection or mutation."""

    def __init__(self):
        raise TypeError("Use DetectedAudioServer.detect()")

    @classmethod
    def detect(cls, inherited):
        address = inherited.get("PULSE_SERVER")
        if address is None:
            runtime = inherited.get("XDG_RUNTIME_DIR", f"/run/user/{os.getuid()}")
            address = f"unix:{runtime}/pulse/native"
        if not address.startswith("unix:/") or any(char.isspace() for char in address):
            raise RuntimeError("PULSE_SERVER must be a single absolute local unix: socket path.")
        path = Path(address.removeprefix("unix:"))
        if not path.is_socket():
            raise RuntimeError(f"No PipeWire Pulse socket at {path}; start the environment's "
                               "pipewire, pipewire-pulse and WirePlumber services before testing.")
        server = object.__new__(cls)
        server.__path = path
        status = path.stat()
        server.__identity = (status.st_dev, status.st_ino)
        return server

    @property
    def address(self):
        return f"unix:{self.__path}"

    def check(self):
        status = self.__path.stat()
        if not self.__path.is_socket() or (status.st_dev, status.st_ino) != self.__identity:
            raise RuntimeError("The selected PipeWire Pulse socket was replaced; stop and rerun the test.")


def isolated_environment(runtime, inherited, audio_server, sink):
    """Keep only the selected audio endpoint; isolate display, settings and bus."""
    excluded = {
        "DISPLAY", "WAYLAND_DISPLAY", "WAYLAND_SOCKET", "XAUTHORITY",
        "SESSION_MANAGER", "DESKTOP_STARTUP_ID", "WESTON_CONFIG_FILE",
        "QT_QUICK_BACKEND", "QSG_RHI_BACKEND", "QT_QPA_PLATFORM",
        "QT_QPA_PLATFORMTHEME", "QT_QPA_PLATFORM_PLUGIN_PATH",
    }
    env = {key: value for key, value in inherited.items()
           if key not in excluded and not key.startswith(
               ("PULSE_", "PIPEWIRE_", "DBUS_", "XDG_SESSION_", "NAGAMETV_GUI_"))}
    env.update({
        "XDG_RUNTIME_DIR": str(runtime),
        "XDG_CONFIG_HOME": str(runtime / "config"),
        "XDG_CACHE_HOME": str(runtime / "cache"),
        "XDG_STATE_HOME": str(runtime / "state"),
        "XDG_DATA_HOME": str(runtime / "data"),
        "XDG_CURRENT_DESKTOP": "weston",
        "PULSE_SERVER": audio_server.address,
        "PULSE_SINK": sink,
        "PULSE_SOURCE": f"{sink}.monitor",
        "PULSE_CLIENTCONFIG": str(runtime / "pulse-client.conf"),
        # These are stream properties: disappearing targets must not route test
        # audio to a physical output on the shared PipeWire server.
        "PULSE_PROP": "node.dont-fallback=true node.dont-move=true node.dont-reconnect=true",
        "DBUS_SESSION_BUS_ADDRESS": f"unix:path={runtime}/bus",
        "QT_QPA_PLATFORM": "xcb",
        "QSG_RHI_BACKEND": "opengl",
        "NAGAMETV_AUDIO_SINK": "pulsesink",
    })
    return env


def detect_resources():
    if sys.platform != "linux":
        raise RuntimeError("Isolated GUI tests require Linux (DRM, Weston and a running PipeWire Pulse server).")
    required = ("weston", "Xwayland", "openbox", "pactl", "parec",
                "dbus-daemon", "dbus-send", "xdpyinfo", "xprop", "glxinfo", "gst-launch-1.0")
    missing = [name for name in required if shutil.which(name) is None]
    if missing:
        raise RuntimeError(f"Missing test dependencies: {', '.join(missing)}; see docs/gui-test-environment.md.")
    devices = [*Path("/dev/dri").glob("renderD*"), Path("/dev/nvidiactl")]
    if not any(path.exists() and os.access(path, os.R_OK | os.W_OK) for path in devices):
        raise RuntimeError("No accessible GPU render device; connect system:gpu. Software rendering is not allowed.")


def stop_process_group(process):
    """Also stop descendants after the group leader has already exited."""
    try:
        os.killpg(process.pid, signal.SIGTERM)
    except ProcessLookupError:
        pass
    try:
        process.wait(timeout=STOP_TIMEOUT_SECONDS)
    except subprocess.TimeoutExpired:
        pass
    finally:
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        process.wait()


class _Processes:
    def __init__(self, stack, env, logs):
        self._stack = stack
        self.env = env
        self.logs = logs
        self.services = []
        self.validators = []

    def start(self, name, command, *, output=None, pass_fds=()):
        log = self._stack.enter_context((self.logs / f"{name}.log").open("wb"))
        process = subprocess.Popen(command, env=self.env, cwd=ROOT,
                                   stdin=subprocess.DEVNULL, stdout=output or log,
                                   stderr=log, start_new_session=True, pass_fds=pass_fds)
        self._stack.callback(stop_process_group, process)
        self.services.append((name, process))
        return process

    def check(self):
        for name, process in self.services:
            if process.poll() is not None:
                raise RuntimeError(f"{name} exited with {process.returncode}; see {self.logs / (name + '.log')}")
        for validate in self.validators:
            validate()

    def until(self, description, predicate):
        deadline = time.monotonic() + START_TIMEOUT_SECONDS
        while True:
            self.check()
            result = predicate()
            if result:
                return result
            if time.monotonic() >= deadline:
                raise RuntimeError(f"Timed out waiting for {description}")
            time.sleep(POLL_SECONDS)

    def probe(self, *command):
        self.check()
        process = subprocess.Popen(command, env=self.env, cwd=ROOT, text=True,
                                   stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                                   stderr=subprocess.PIPE, start_new_session=True)
        try:
            stdout, stderr = process.communicate(timeout=PROBE_TIMEOUT_SECONDS)
            if process.returncode:
                raise subprocess.CalledProcessError(process.returncode, command, stdout, stderr)
        finally:
            stop_process_group(process)
            process.stdout.close()
            process.stderr.close()
        self.check()
        return stdout


def validate_renderer(info):
    if "direct rendering: Yes" not in info or "OpenGL renderer string:" not in info:
        raise RuntimeError("Xwayland did not report direct OpenGL rendering.")
    if any(name in info.lower() for name in SOFTWARE_RENDERERS):
        raise RuntimeError("Software OpenGL renderer detected; a working GPU is required.")
    # Mesa exposes this field; proprietary NVIDIA drivers may omit it.
    if re.search(r"Accelerated:\s*no", info, re.IGNORECASE):
        raise RuntimeError("OpenGL acceleration is disabled.")


def owned_audio_modules(client, sink):
    # pactl's JSON module records omit the numeric ID. Short output includes
    # it; skip continuation lines from unrelated modules' multiline arguments.
    modules = []
    for line in client.probe("pactl", "list", "short", "modules").splitlines():
        fields = line.split("\t", 3)
        if (len(fields) >= 3 and fields[0].isdigit() and fields[1] == "module-null-sink"
                and f"sink_name={sink}" in shlex.split(fields[2])):
            modules.append(int(fields[0]))
    return modules


def release_audio_sink(client, server, sink):
    # Never unload a bare remembered ID: a restarted shared server may have
    # reused it for someone else's module. Also covers failed/timed-out loads.
    server.check()
    for module in owned_audio_modules(client, sink):
        client.probe("pactl", "unload-module", str(module))


def audio_sink_info(client, sink, module):
    sinks = json.loads(client.probe("pactl", "--format=json", "list", "sinks"))
    matches = [item for item in sinks if item["name"] == sink]
    if not matches:
        return None
    if (len(matches) != 1 or matches[0].get("owner_module") != module
            or matches[0].get("properties", {}).get("factory.name") != "support.null-audio-sink"):
        raise RuntimeError("The test output is not the owned PipeWire null sink.")
    return matches[0]


class ValidatedAudio:
    """A lease on our virtual output; the external server is never owned here."""

    def __init__(self):
        raise TypeError("Use ValidatedAudio.open()")

    @classmethod
    @contextmanager
    def open(cls, server, env, runtime, logs):
        # Pin the checked endpoint for the entire lease, including cleanup.
        env = dict(env)
        if env.get("PULSE_SERVER") != server.address:
            raise RuntimeError("Audio environment does not match the detected server.")
        sink = env["PULSE_SINK"]
        with ExitStack() as stack:
            client = _Processes(stack, env, logs)
            server.check()
            info = json.loads(client.probe("pactl", "--format=json", "info"))
            if "PipeWire" not in info.get("server_name", ""):
                raise RuntimeError("GUI tests require the environment's PipeWire Pulse server.")
            (logs / "audio-server.json").write_text(json.dumps(info, indent=2) + "\n")
            # Register before load so that a lost reply cannot leak our module.
            stack.callback(release_audio_sink, client, server, sink)
            module = int(client.probe("pactl", "load-module", "module-null-sink",
                f"sink_name={sink}", f"rate={AUDIO_RATE}", f"channels={AUDIO_CHANNELS}",
                "sink_properties='node.virtual=true priority.session=0'"))
            output = client.until("test null sink", lambda: audio_sink_info(client, sink, module))
            validate_audio(client, runtime, sink, output["monitor_source"])
            audio = object.__new__(cls)
            audio.__client = client
            audio.__server = server
            audio.__sink = sink
            audio.__module = module
            audio.__next_check = 0
            try:
                yield audio
            finally:
                audio.__client = None

    def check(self):
        if self.__client is None:
            raise RuntimeError("The validated audio output has already closed.")
        if time.monotonic() < self.__next_check:
            return
        self.__server.check()
        if audio_sink_info(self.__client, self.__sink, self.__module) is None:
            raise RuntimeError("The test audio output disappeared; stopping the test.")
        self.__next_check = time.monotonic() + AUDIO_CHECK_INTERVAL_SECONDS


def monitor_connected(client, monitor_name, recorder_pid):
    # Sink JSON identifies its monitor by name, while source-output JSON uses
    # the source's numeric index. Resolve the name before comparing streams.
    sources = json.loads(client.probe("pactl", "--format=json", "list", "sources"))
    monitor_indices = {item["index"] for item in sources if item["name"] == monitor_name}
    return any(item.get("source") in monitor_indices
               and item.get("properties", {}).get("application.process.id") == str(recorder_pid)
               for item in json.loads(client.probe("pactl", "--format=json", "list", "source-outputs")))


def validate_audio(processes, runtime, sink, monitor_name):
    capture = processes.logs / "audio-probe.raw"
    # The recorder must be connected before the test tone is sent. Observe the
    # monitor stream, not just a successful server connection or an empty sink.
    with ExitStack() as stack:
        recording = _Processes(stack, processes.env, processes.logs)
        output = stack.enter_context(capture.open("wb"))
        recorder = recording.start("audio-monitor", ["parec", "--raw", "--format=s16le",
                                   f"--rate={AUDIO_RATE}", f"--channels={AUDIO_CHANNELS}",
                                   f"--latency-msec={AUDIO_MONITOR_LATENCY_MS}",
                                   f"--device={sink}.monitor"], output=output)
        recording.until("test audio monitor connection",
                        lambda: monitor_connected(processes, monitor_name, recorder.pid))
        processes.probe("gst-launch-1.0", "-q", "audiotestsrc", f"num-buffers={AUDIO_PROBE_BUFFERS}",
                        f"samplesperbuffer={AUDIO_RATE // AUDIO_PROBE_BUFFERS}", "wave=sine", "!",
                        f"audio/x-raw,rate={AUDIO_RATE},channels={AUDIO_CHANNELS}",
                        "!", "audioconvert", "!", "pulsesink", f"device={sink}")
    samples = array("h")
    samples.frombytes(capture.read_bytes())
    if not samples or max(abs(value) for value in samples) < MIN_AUDIO_PROBE_PEAK:
        raise RuntimeError("No test tone received through the virtual audio monitor.")
    print(f"Audio validated: GStreamer pulsesink -> PipeWire {sink} -> monitor.", flush=True)


def _run_command(processes, command):
    processes.check()
    process = subprocess.Popen(command, cwd=ROOT, env=processes.env, start_new_session=True)
    try:
        while process.poll() is None:
            processes.check()
            time.sleep(POLL_SECONDS)
        processes.check()
        return process.returncode if process.returncode >= 0 else SIGNAL_EXIT_BASE - process.returncode
    finally:
        stop_process_group(process)


class ValidatedSession:
    """Only open() publishes a session, after display/GPU/audio validation.

    The context owns the display services and a virtual audio output. No
    environment or mutation API is exposed to callers; run() rechecks resource
    liveness during each command.
    """

    def __init__(self):
        raise TypeError("Use ValidatedSession.open()")

    @classmethod
    @contextmanager
    def open(cls, logs):
        detect_resources()
        audio_server = DetectedAudioServer.detect(os.environ)
        with tempfile.TemporaryDirectory(prefix="nagametv-gui-", dir="/tmp") as directory, ExitStack() as stack:
            runtime = Path(directory)
            sink = AUDIO_SINK_PREFIX + uuid.uuid4().hex
            env = isolated_environment(runtime, os.environ, audio_server, sink)
            (runtime / "pulse-client.conf").write_text("autospawn = no\nenable-shm = no\n")
            processes = _Processes(stack, env, logs)
            audio = stack.enter_context(ValidatedAudio.open(audio_server, env, runtime, logs))
            processes.validators.append(audio.check)
            processes.start("dbus", ["dbus-daemon", "--session", "--nofork",
                                    f"--address={env['DBUS_SESSION_BUS_ADDRESS']}"])
            processes.until("private D-Bus socket", (runtime / "bus").is_socket)
            processes.probe("dbus-send", "--session", "--print-reply", "--dest=org.freedesktop.DBus",
                            "/org/freedesktop/DBus", "org.freedesktop.DBus.ListNames")
            # A rootful Xwayland has its own X11 window manager. Unlike Weston's
            # rootless XWM, Openbox restores focus after dialogs even with no
            # physical input seat on this headless compositor.
            processes.start("weston", ["weston", "--no-config", "--backend=headless",
                "--renderer=gl", "--shell=kiosk-shell.so", "--idle-time=0", "--socket=nagametv-wayland",
                f"--width={OUTPUT_WIDTH}", f"--height={OUTPUT_HEIGHT}"])
            processes.until("private Wayland socket", (runtime / "nagametv-wayland").is_socket)
            env["WAYLAND_DISPLAY"] = "nagametv-wayland"
            # -displayfd allocates a free X display and signals readiness. Never
            # guess a number or connect to a display inherited from the caller.
            number_file = runtime / "display-number"
            with number_file.open("wb") as display_output:
                processes.start("xwayland", ["Xwayland", "-displayfd", str(display_output.fileno()),
                    "-nolisten", "tcp", "-noreset", "-glamor", "gl", "-fullscreen",
                    "-geometry", f"{OUTPUT_WIDTH}x{OUTPUT_HEIGHT}"], pass_fds=(display_output.fileno(),))
                processes.until("Xwayland readiness", lambda: number_file.stat().st_size > 0)
            number = number_file.read_text().strip()
            if not re.fullmatch(r"\d+", number):
                raise RuntimeError(f"Invalid private Xwayland display number: {number!r}")
            display = f":{number}"
            (runtime / "display").write_text(display)
            env["DISPLAY"] = display
            processes.probe("xdpyinfo", "-display", display)
            wm_config = runtime / "openbox.xml"
            wm_config.write_text('<openbox_config xmlns="http://openbox.org/3.4/rc">'
                '<focus><focusNew>yes</focusNew><followMouse>no</followMouse></focus>'
                '<desktops><number>1</number></desktops></openbox_config>')
            processes.start("openbox", ["openbox", "--sm-disable", "--config-file", str(wm_config)])
            processes.until("X11 window manager", lambda: "window id #" in processes.probe(
                "xprop", "-root", "_NET_SUPPORTING_WM_CHECK"))
            graphics = processes.probe("glxinfo", "-B")
            (logs / "graphics.log").write_text(graphics)
            validate_renderer(graphics)
            compositor_log = (logs / "weston.log").read_text()
            renderer = re.search(r"GL renderer: (.+)", compositor_log)
            if renderer is None or any(name in renderer[1].lower() for name in SOFTWARE_RENDERERS):
                raise RuntimeError("Weston did not initialize a hardware GL renderer.")
            print(graphics, flush=True)
            (runtime / "pulse-server").write_text(audio_server.address)
            (runtime / "pulse-sink").write_text(sink)
            env["NAGAMETV_GUI_SESSION_DIR"] = str(runtime)
            session = object.__new__(cls)
            session.__processes = processes
            try:
                yield session
            finally:
                # A retained reference cannot launch anything after context exit.
                session.__processes = None

    def run(self, command):
        processes = self.__processes
        if processes is None:
            raise RuntimeError("The validated GUI session has already closed.")
        return _run_command(processes, command)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="validate the isolated display and audio, then exit")
    parser.add_argument("command", nargs=argparse.REMAINDER, help="command to run, preceded by --")
    args = parser.parse_args()
    command = args.command[1:] if args.command[:1] == ["--"] else args.command
    if args.check == bool(command):
        parser.error("select either --check or -- COMMAND [ARG ...]")
    log_root = ROOT / "build" / "gui-tests"
    log_root.mkdir(parents=True, exist_ok=True)
    logs = Path(tempfile.mkdtemp(prefix="session-", dir=log_root))
    print(f"Isolated GUI session logs: {logs}", flush=True)
    try:
        with ValidatedSession.open(logs) as session:
            return session.run(command) if command else 0
    except KeyboardInterrupt:
        return SIGNAL_EXIT_BASE + signal.SIGINT
    except (OSError, RuntimeError, ValueError, subprocess.SubprocessError) as error:
        print(f"Isolated GUI tests failed: {error}", file=sys.stderr)
        if isinstance(error, subprocess.CalledProcessError):
            print(error.stderr, file=sys.stderr)
        print(f"Session logs: {logs}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    # SIGTERM should unwind the same lifetime scope as an interrupted test.
    signal.signal(signal.SIGTERM, lambda *_: sys.exit(SIGNAL_EXIT_BASE + signal.SIGTERM))
    sys.exit(main())
