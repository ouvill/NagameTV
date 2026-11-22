"""Wait for the viewer to exit before systemd stops the Wayland compositor."""
import argparse
import os
from pathlib import Path
import select
import signal
import sys


def stop(runtime, compositor):
    # Set this before reading the PID so a session still starting will see it.
    (runtime / "stopping").touch()
    try:
        viewer = int((runtime / "viewer.pid").read_text())
    except FileNotFoundError:
        return  # The launcher has yet to publish its PID.
    if viewer <= 1 or viewer == compositor:
        raise ValueError(f"Invalid viewer PID {viewer}")

    try:
        pidfd = os.pidfd_open(viewer)
    except ProcessLookupError:
        return  # labwc reaps the viewer on exit.
    try:
        # The launcher execs the viewer as labwc's primary client. Refuse a
        # stale PID that now belongs to a process outside this session.
        try:
            status = Path(f"/proc/{viewer}/status").read_text(errors="replace")
            parent = next(line.split()[1] for line in status.splitlines() if line.startswith("PPid:"))
            if int(parent) != compositor:
                raise ValueError(f"Viewer PID {viewer} is no longer a child of labwc PID {compositor}")
            signal.pidfd_send_signal(pidfd, signal.SIGTERM)
        except (FileNotFoundError, ProcessLookupError):
            return  # The viewer exited while the stop command was checking it.
        print(f"NagameTV kiosk: waiting for viewer PID {viewer} to exit", file=sys.stderr, flush=True)
        # TimeoutStopSec bounds this synchronous stop and the cgroup cleanup.
        wait = select.poll()
        wait.register(pidfd, select.POLLIN)
        wait.poll()
    finally:
        os.close(pidfd)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("compositor", nargs="?", type=int)
    args = parser.parse_args()
    if args.compositor is None:
        return 0  # systemd has already observed the compositor exit.
    try:
        stop(Path(os.environ["RUNTIME_DIRECTORY"]), args.compositor)
    except (KeyError, OSError, ValueError) as error:
        print(f"NagameTV kiosk: ordered shutdown failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
