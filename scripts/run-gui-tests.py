#!/usr/bin/env python3
"""Linux: run GUI tests on a private GPU display and clocked virtual audio sink."""

import argparse
from array import array
from contextlib import contextmanager, ExitStack
import json
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import time

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
AUDIO_SINK = "nagametv_test"
AUDIO_PROBE_BUFFERS = 10
MIN_AUDIO_PROBE_PEAK = 1000
PULSE_COOKIE_BYTES = 256
SOFTWARE_RENDERERS = ("llvmpipe", "softpipe", "software rasterizer", "swrast")


def isolated_environment(runtime, inherited):
    """Never inherit a route to the user's display, audio, or session bus."""
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
        "PULSE_SERVER": f"unix:{runtime}/pulse.sock",
        "PULSE_SINK": AUDIO_SINK,
        "PULSE_SOURCE": f"{AUDIO_SINK}.monitor",
        "PULSE_RUNTIME_PATH": str(runtime / "pulse"),
        "PULSE_CLIENTCONFIG": str(runtime / "pulse-client.conf"),
        "PULSE_COOKIE": str(runtime / "pulse.cookie"),
        "DBUS_SESSION_BUS_ADDRESS": f"unix:path={runtime}/bus",
        "QT_QPA_PLATFORM": "xcb",
        "QSG_RHI_BACKEND": "opengl",
        "NAGAMETV_AUDIO_SINK": "pulsesink",
    })
    return env


def detect_resources():
    if sys.platform != "linux":
        raise RuntimeError("Isolated GUI tests require Linux (DRM, Weston and PulseAudio).")
    required = ("weston", "Xwayland", "openbox", "pulseaudio", "pactl", "parec",
                "dbus-daemon", "dbus-send", "xdpyinfo", "xprop", "glxinfo", "gst-launch-1.0")
    missing = [name for name in required if shutil.which(name) is None]
    if missing:
        raise RuntimeError(f"Missing test dependencies: {', '.join(missing)}; update the Workshop SDK.")
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

    def until(self, description, predicate):
        deadline = time.monotonic() + START_TIMEOUT_SECONDS
        while True:
            self.check()
            if predicate():
                return
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


def validate_audio(processes, runtime):
    sinks = json.loads(processes.probe("pactl", "--format=json", "list", "sinks"))
    if len(sinks) != 1 or sinks[0]["name"] != AUDIO_SINK or sinks[0]["driver"] != "module-null-sink.c":
        raise RuntimeError("Expected exactly one private PulseAudio null sink.")
    capture = runtime / "audio-probe.raw"
    # The recorder must be connected before the test tone is sent. Observe the
    # monitor stream, not just a successful server connection or an empty sink.
    with ExitStack() as stack:
        recording = _Processes(stack, processes.env, processes.logs)
        output = stack.enter_context(capture.open("wb"))
        recording.start("audio-monitor", ["parec", "--raw", "--format=s16le",
                        f"--rate={AUDIO_RATE}", f"--channels={AUDIO_CHANNELS}",
                        f"--device={AUDIO_SINK}.monitor"], output=output)
        recording.until("audio monitor connection", lambda: bool(json.loads(
            processes.probe("pactl", "--format=json", "list", "source-outputs"))))
        processes.probe("gst-launch-1.0", "-q", "audiotestsrc", f"num-buffers={AUDIO_PROBE_BUFFERS}",
                        f"samplesperbuffer={AUDIO_RATE // AUDIO_PROBE_BUFFERS}", "wave=sine", "!",
                        f"audio/x-raw,rate={AUDIO_RATE},channels={AUDIO_CHANNELS}",
                        "!", "audioconvert", "!", "pulsesink", f"device={AUDIO_SINK}")
    samples = array("h")
    samples.frombytes(capture.read_bytes())
    if not samples or max(abs(value) for value in samples) < MIN_AUDIO_PROBE_PEAK:
        raise RuntimeError("No test tone received through the virtual audio monitor.")
    print("Audio validated: GStreamer pulsesink -> private null sink -> monitor.", flush=True)


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

    The context owns all service lifetimes. No environment or mutation API is
    exposed to callers; run() rechecks service liveness during each command.
    """

    def __init__(self):
        raise TypeError("Use ValidatedSession.open()")

    @classmethod
    @contextmanager
    def open(cls, logs):
        detect_resources()
        with tempfile.TemporaryDirectory(prefix="nagametv-gui-", dir="/tmp") as directory, ExitStack() as stack:
            runtime = Path(directory)
            env = isolated_environment(runtime, os.environ)
            (runtime / "pulse").mkdir(mode=0o700)
            (runtime / "pulse-client.conf").write_text("autospawn = no\nenable-shm = no\n")
            (runtime / "pulse.cookie").write_bytes(os.urandom(PULSE_COOKIE_BYTES))
            processes = _Processes(stack, env, logs)
            processes.start("dbus", ["dbus-daemon", "--session", "--nofork",
                                    f"--address={env['DBUS_SESSION_BUS_ADDRESS']}"])
            processes.until("private D-Bus socket", (runtime / "bus").is_socket)
            processes.probe("dbus-send", "--session", "--print-reply", "--dest=org.freedesktop.DBus",
                            "/org/freedesktop/DBus", "org.freedesktop.DBus.ListNames")
            pulse_config = runtime / "pulse.pa"
            pulse_config.write_text(
                f".fail\nload-module module-native-protocol-unix socket={runtime}/pulse.sock auth-cookie={runtime}/pulse.cookie\n"
                f"load-module module-null-sink sink_name={AUDIO_SINK} rate={AUDIO_RATE} channels={AUDIO_CHANNELS}\n"
                f"set-default-sink {AUDIO_SINK}\nset-default-source {AUDIO_SINK}.monitor\n")
            processes.start("pulse", ["pulseaudio", "-n", f"--file={pulse_config}",
                                      "--daemonize=no", "--use-pid-file=no", "--exit-idle-time=-1",
                                      "--realtime=no", "--high-priority=no", "--disable-shm=yes",
                                      "--log-target=stderr"])
            processes.until("private PulseAudio socket", (runtime / "pulse.sock").is_socket)
            validate_audio(processes, runtime)
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
