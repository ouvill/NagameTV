"""Wait for the viewer to exit before systemd stops the Wayland compositor."""
import argparse
from contextlib import ExitStack
import os
from pathlib import Path
import pwd
import select
import signal
import stat
import sys


MAX_PID_RECORD = 64


def drop_privileges():
    if os.geteuid() != 0:
        return
    account = pwd.getpwnam("nagametv")
    if account.pw_uid == 0 or account.pw_gid == 0:
        raise ValueError("The kiosk account must be unprivileged")
    # Clear saved IDs and supplementary groups before accessing kiosk-owned paths.
    os.setgroups([])
    os.setresgid(account.pw_gid, account.pw_gid, account.pw_gid)
    os.setresuid(account.pw_uid, account.pw_uid, account.pw_uid)


def request_stop(directory):
    try:
        fd = os.open("stopping", os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600, dir_fd=directory)
    except FileExistsError:
        return  # The launcher treats any existing entry as a stop request.
    os.close(fd)


def read_viewer(directory):
    fd = os.open("viewer.pid", os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=directory)
    try:
        if not stat.S_ISREG(os.fstat(fd).st_mode):
            raise ValueError("The viewer PID record must be a regular file")
        record = os.read(fd, MAX_PID_RECORD + 1)
    finally:
        os.close(fd)
    fields = record.split()
    if len(record) > MAX_PID_RECORD or len(fields) != 3 or not all(field.isdigit() for field in fields):
        raise ValueError("Invalid viewer PID record")
    return tuple(map(int, fields))


def process_identity(pid):
    # The command name can contain spaces or parentheses. PPid and starttime
    # are fields 4 and 22 of /proc/PID/stat, after the parenthesized name.
    fields = Path(f"/proc/{pid}/stat").read_bytes().rpartition(b") ")[2].split()
    if len(fields) < 20:
        raise ValueError(f"Invalid process identity for PID {pid}")
    return int(fields[1]), int(fields[19])


def watch_exit(pidfd):
    wait = select.poll()
    wait.register(pidfd, select.POLLIN)
    return wait


def stop(runtime, compositor):
    if compositor <= 1:
        raise ValueError(f"Invalid labwc PID {compositor}")
    with ExitStack() as descriptors:
        directory = os.open(runtime, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
        descriptors.callback(os.close, directory)
        # Publish the stop request before reading the PID so startup can see it.
        request_stop(directory)
        try:
            viewer, viewer_started, compositor_started = read_viewer(directory)
        except FileNotFoundError:
            return  # The launcher has yet to publish its PID.
        if viewer <= 1 or viewer == compositor:
            raise ValueError(f"Invalid viewer PID {viewer}")

        try:
            compositor_fd = os.pidfd_open(compositor)
            descriptors.callback(os.close, compositor_fd)
            viewer_fd = os.pidfd_open(viewer)
            descriptors.callback(os.close, viewer_fd)
            viewer_exit = watch_exit(viewer_fd)

            parent, actual_viewer_start = process_identity(viewer)
            _, actual_compositor_start = process_identity(compositor)
            # If either pinned process exited during the /proc reads, the
            # numeric paths could have described a replacement process.
            if viewer_exit.poll(0) or watch_exit(compositor_fd).poll(0):
                return
            if parent != compositor:
                raise ValueError(f"Viewer PID {viewer} is no longer a child of labwc PID {compositor}")
            if (actual_viewer_start, actual_compositor_start) != (viewer_started, compositor_started):
                raise ValueError("Viewer or labwc PID was reused")
            signal.pidfd_send_signal(viewer_fd, signal.SIGTERM)
        except (FileNotFoundError, ProcessLookupError):
            return  # A process exited before validation or signal delivery.

        print(f"NagameTV kiosk: waiting for viewer PID {viewer} to exit", file=sys.stderr, flush=True)
        # TimeoutStopSec bounds this synchronous stop and the cgroup cleanup.
        viewer_exit.poll()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("compositor", nargs="?", type=int)
    args = parser.parse_args()
    if args.compositor is None:
        return 0  # systemd has already observed the compositor exit.
    try:
        drop_privileges()
        stop(Path(os.environ["RUNTIME_DIRECTORY"]), args.compositor)
    except (KeyError, OSError, OverflowError, ValueError) as error:
        print(f"NagameTV kiosk: ordered shutdown failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
