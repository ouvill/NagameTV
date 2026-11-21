"""Recover the Pi kiosk's HDMI0 sink in its own PipeWire session."""

import json
import os
from pathlib import Path
import subprocess
import sys
import time


SINK = "nagametv-hdmi0"
DRM = Path("/sys/class/drm")
POLL_SECONDS = 2
SETTLE_SECONDS = 2
RETRY_SECONDS = 6
MAX_ATTEMPTS = 3


def connector_ready(drm):
    for connector in drm.glob("card*-HDMI-A-1"):
        try:
            if (connector.joinpath("status").read_text().strip() == "connected"
                    and connector.joinpath("enabled").read_text().strip() == "enabled"
                    and connector.joinpath("edid").read_bytes()):
                return True
        except FileNotFoundError:
            # A DRM device can disappear between discovery and the sysfs reads.
            continue
    return False


def run(*args):
    return subprocess.run(args, check=True, capture_output=True, text=True, timeout=5).stdout


class Recovery:
    def __init__(self, clock=time.monotonic):
        self.clock = clock
        self.sink_index = None
        self.retry_at = None
        self.attempts = 0
        self.last_message = None

    def report(self, message):
        if message != self.last_message:
            print(f"NagameTV HDMI audio: {message}", file=sys.stderr, flush=True)
            self.last_message = message

    def check(self):
        sinks = json.loads(run("pactl", "--format=json", "list", "sinks"))
        sink = next((item for item in sinks if item["name"] == SINK), None)
        if sink is not None:
            self.retry_at = None
            self.attempts = 0
            if self.sink_index != sink["index"]:
                # WirePlumber's route volume can start at 40% on a new sink.
                # The TV and the app own the loudness for this output.
                run("pactl", "set-sink-volume", str(sink["index"]), "100%")
                run("pactl", "set-default-sink", SINK)
                self.sink_index = sink["index"]
                self.report(f"HDMI0 sink {SINK} is ready")
            return

        self.sink_index = None
        if not connector_ready(DRM):
            self.report("HDMI0 sink missing. Waiting for HDMI-A-1 to be connected, enabled and have EDID")
            self.retry_at = None
            self.attempts = 0
            return

        now = self.clock()
        if self.retry_at is None:
            self.report("HDMI0 sink missing with HDMI-A-1 ready. Waiting for the audio graph to settle")
            self.retry_at = now + SETTLE_SECONDS
        elif now >= self.retry_at:
            if self.attempts == MAX_ATTEMPTS:
                self.report(f"HDMI0 sink still missing after {MAX_ATTEMPTS} attempts. "
                            "Reconnect HDMI0 or inspect WirePlumber")
                return
            self.attempts += 1
            self.retry_at = now + RETRY_SECONDS
            self.report(f"HDMI0 sink missing. Restarting kiosk WirePlumber (attempt {self.attempts}/{MAX_ATTEMPTS})")
            # ACP does not re-emit the failed node on jack availability alone.
            # Enumerating the card again also covers a missing HDMI profile.
            # Leave PipeWire running so the Pulse client can relink.
            run("systemctl", "--user", "restart", "wireplumber.service")


def main():
    # The system unit runs as the kiosk user without opening another PAM session.
    runtime = f"/run/user/{os.getuid()}"
    os.environ.update(XDG_RUNTIME_DIR=runtime, PULSE_SERVER=f"unix:{runtime}/pulse/native",
                      DBUS_SESSION_BUS_ADDRESS=f"unix:path={runtime}/bus")
    recovery = Recovery()
    while True:
        try:
            recovery.check()
        except subprocess.CalledProcessError as error:
            recovery.report(f"{error}: {error.stderr.strip()}")
        except (OSError, ValueError, subprocess.TimeoutExpired) as error:
            recovery.report(f"HDMI0 check failed: {error}")
        time.sleep(POLL_SECONDS)


if __name__ == "__main__":
    main()
