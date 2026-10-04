"""Qt suite definitions and preparation; invoke through scripts/test.py."""
import argparse
from dataclasses import dataclass
from enum import Enum
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import tempfile
import time

from scripts.build.support import ROOT, lock_fds
from scripts.testing.gui_session import require_inherited_session, stop_process_group


class Requirement(Enum):
    CPU = "cpu"
    GUI = "gui"


@dataclass(frozen=True)
class Suite:
    requirement: Requirement
    arguments: tuple[str, ...]


SUITES = {
    "connection": Suite(Requirement.CPU, ("--native-tests", "connection")),
    "desktop-media": Suite(Requirement.CPU, ("--native-tests", "desktop-media")),
    "localization": Suite(Requirement.CPU, ("--native-tests", "localization")),
    "subtitle-outline": Suite(Requirement.CPU, ("--native-tests", "subtitle-outline")),
    "danmaku": Suite(Requirement.GUI, ("--qml-tests",)),
    "ui-style": Suite(Requirement.GUI, ("--native-tests", "channel-views", "-input", "tests/ui-gallery")),
    "channel-wheel": Suite(Requirement.GUI, ("--native-tests", "channel-views", "-input", "tests/channel-wheel")),
    "startup": Suite(Requirement.GUI, ("--native-tests", "startup")),
    "screenshot": Suite(Requirement.GUI, ("--native-tests", "screenshots")),
    "video-item": Suite(Requirement.GUI, ("--video-item-tests",)),
    "pointer-activity": Suite(Requirement.GUI, ("--native-tests", "pointer-activity")),
    "portal-dialogs": Suite(Requirement.GUI, ("--native-tests", "portal-dialogs")),
    "subtitle-rendering": Suite(Requirement.GUI, ("--native-tests", "subtitle-rendering", "-input", "tests/subtitles")),
    "ui-capture": Suite(Requirement.GUI, ("--native-tests", "channel-views", "-input", "tests/ui-review")),
}
STARTUP_CASES = ("startup", "recording-pid-change", "timeshift", "data-broadcast", "screenshot-playback", "video-processing")
RECORDING_CASES = ("recording-audit", "recording-probe")
PORTAL_TIMEOUT_SECONDS = 5
PORTAL_POLL_SECONDS = 0.05


def validate_arguments(name, arguments):
    if name != "danmaku" and "--evaluation-legacy-comments" in arguments:
        raise ValueError("--evaluation-legacy-comments requires the danmaku suite")
    if name == "startup":
        if not arguments or (arguments[0] in STARTUP_CASES and len(arguments) == 1):
            return
        if (arguments[0] in RECORDING_CASES and len(arguments) == 2
                and Path(arguments[1]).is_file()):
            return
        raise ValueError(f"startup expects one of {', '.join(STARTUP_CASES)}, or recording-audit/probe TS_PATH")
    if name == "danmaku":
        if arguments not in ([], ["--evaluation-legacy-comments"]):
            raise ValueError("danmaku accepts only --evaluation-legacy-comments")
    elif name in ("connection", "desktop-media", "localization", "video-item", "portal-dialogs") and arguments:
        raise ValueError(f"{name} does not accept additional arguments")
    elif "--ui-only" in arguments:
        raise ValueError("--ui-only was retired; subtitle-rendering uses the validated private GPU session")


def run_suite(name, arguments):
    validate_arguments(name, arguments)
    suite = SUITES[name]
    if suite.requirement == Requirement.GUI:
        require_inherited_session(os.environ)
    binary = os.environ.get("NAGAMETV_TEST_BINARY")
    if not binary or not Path(binary).is_file():
        raise RuntimeError("Missing prepared test binary; use python3 scripts/test.py SUITE")
    with tempfile.TemporaryDirectory(prefix=f"nagametv-{name}-") as directory:
        temporary = Path(directory)
        env = {key: value for key, value in os.environ.items() if key not in (
            "NAGAMETV_SERVER", "NAGAMETV_SERVICE_ID", "NAGAMETV_AUTOPLAY",
            "NAGAMETV_REMOTE_ENABLED", "NAGAMETV_REMOTE_ADDR", "NAGAMETV_REMOTE_PORT")}
        env.update({f"XDG_{kind}_HOME": str(temporary / kind.lower())
                    for kind in ("CONFIG", "CACHE", "STATE", "DATA")})
        env["NAGAMETV_DIAGNOSTICS"] = "0"
        command = [binary, *suite.arguments]
        if name == "startup":
            qpa = env.get("NAGAMETV_TEST_QPA", "wayland" if
                          env.get("NAGAMETV_DEINTERLACE", "yadif").lower() == "va" else "xcb")
            if qpa not in ("xcb", "wayland", "auto"):
                raise ValueError("NAGAMETV_TEST_QPA must be xcb, wayland or auto")
            if qpa == "auto":
                env.pop("QT_QPA_PLATFORM", None)
            else:
                env["QT_QPA_PLATFORM"] = qpa
            if arguments:
                command = [binary, "--native-tests", *arguments]
        elif name == "screenshot":
            for relative in ("rust/qml/ScreenshotCapture.qml", "tests/screenshots/tst_ScreenshotCapture.qml"):
                destination = temporary / relative
                destination.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(ROOT / relative, destination)
            command.extend(["-input", str(temporary / "tests/screenshots"), *arguments])
        elif name == "desktop-media":
            command = ["dbus-run-session", "--", *command]
        elif name == "ui-capture":
            (ROOT / "build/ui-review").mkdir(parents=True, exist_ok=True)
            command.extend(arguments)
        elif name != "danmaku":
            command.extend(arguments)

        def run(command):
            return subprocess.run(command, cwd=ROOT, env=env, pass_fds=lock_fds(), check=False).returncode

        if name == "portal-dialogs":
            # The launcher owns a private bus; this service never joins the host bus.
            (temporary / "録画 #100%.ts").touch()
            (temporary / "キャプチャ #100%").mkdir()
            env["VIEWER_PORTAL_TEST_DIR"] = directory
            service = subprocess.Popen([sys.executable, "-m", "scripts.testing.portal_test_service"],
                                       cwd=ROOT, env=env, start_new_session=True)
            try:
                deadline = time.monotonic() + PORTAL_TIMEOUT_SECONDS
                while not (temporary / "ready").exists():
                    if service.poll() is not None:
                        raise RuntimeError(f"Portal test service exited with {service.returncode}")
                    if time.monotonic() >= deadline:
                        raise RuntimeError("Portal test service did not become ready")
                    time.sleep(PORTAL_POLL_SECONDS)
                return run(command)
            finally:
                stop_process_group(service)
        result = run(command)
        if name == "localization" and result == 0:
            # A separate process preserves the fresh translator state.
            return run([binary, "--native-tests", "missing-catalog"])
        return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("suite", choices=SUITES)
    parser.add_argument("arguments", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    try:
        return run_suite(args.suite, args.arguments)
    except (OSError, RuntimeError, ValueError, subprocess.SubprocessError) as error:
        print(f"Qt suite {args.suite} failed: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    signal.signal(signal.SIGTERM, lambda *_: sys.exit(128 + signal.SIGTERM))
    sys.exit(main())
