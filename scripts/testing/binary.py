#!/usr/bin/env python3
"""Build and copy the Qt test binary; suite execution belongs to the runner."""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

from scripts.build.support import ROOT, build_environment, ensure_lock, lock_fds


def prepare(destination, env, evaluation):
    features = "native_tests,video_item_tests"
    if evaluation:
        features += ",evaluation-legacy-comments"
    command = ["cargo", "build", "--manifest-path", str(ROOT / "rust/Cargo.toml"),
               "--locked", "--profile", env["NAGAMETV_TEST_PROFILE"], "--features", features,
               "--message-format=json-render-diagnostics"]
    result = subprocess.run(command, cwd=ROOT, env=env, pass_fds=lock_fds(),
                            stdout=subprocess.PIPE, text=True, check=True)
    executables = [item["executable"] for line in result.stdout.splitlines()
                   if (item := json.loads(line)).get("reason") == "compiler-artifact"
                   and item.get("executable") and item["target"]["name"] == "nagametv"]
    if len(executables) != 1:
        raise RuntimeError(f"Expected one nagametv executable, got {executables!r}")
    shutil.copy2(executables[0], destination)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--prepare", type=Path, required=True, help="build and copy here, without running")
    parser.add_argument("--evaluation-legacy-comments", action="store_true")
    args = parser.parse_args()
    profile = os.environ.get("NAGAMETV_TEST_PROFILE", "dev")
    if profile not in ("dev", "release"):
        parser.error("NAGAMETV_TEST_PROFILE must be dev or release")
    ensure_lock()
    env = build_environment(profile)
    prepare(args.prepare, env, args.evaluation_legacy_comments)
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, RuntimeError, subprocess.SubprocessError) as error:
        print(f"Qt test build/run failed: {error}", file=sys.stderr)
        sys.exit(1)
