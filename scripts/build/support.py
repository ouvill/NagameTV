"""Shared lock inheritance for build/test entry points (Linux only)."""
import os
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[2]


def lock_fds():
    path = os.environ.get("NAGAMETV_BUILD_LOCK_HELD")
    if path and Path("/proc/self/fd/9").exists() and os.path.samefile("/proc/self/fd/9", path):
        return (9,)
    return ()


def ensure_lock():
    if not lock_fds():
        os.execvp("bash", ["bash", str(ROOT / "scripts/build/with-build-lock.sh"),
                            sys.executable, *sys.orig_argv[1:]])


def build_environment(profile):
    env = dict(os.environ, NAGAMETV_TEST_PROFILE=profile)
    env["CARGO_TARGET_DIR"] = str(Path(env.get("CARGO_TARGET_DIR", ROOT / "build/cargo")).resolve())
    return env
