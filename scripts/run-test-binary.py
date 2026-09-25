#!/usr/bin/env python3
"""Build the Qt test binary once, then run a private copy without Cargo."""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

from test_support import ROOT, build_environment, ensure_lock, lock_fds


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
    parser.add_argument("--prepare", type=Path, help="build and copy here, without running")
    parser.add_argument("--evaluation-legacy-comments", action="store_true")
    parser.add_argument("arguments", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    arguments = args.arguments[1:] if args.arguments[:1] == ["--"] else args.arguments
    profile = os.environ.get("NAGAMETV_TEST_PROFILE", "release")
    if profile not in ("dev", "release"):
        parser.error("NAGAMETV_TEST_PROFILE must be dev or release")
    if not args.prepare and not arguments:
        parser.error("provide --prepare PATH or -- TEST-ARGUMENTS")
    ensure_lock()
    env = build_environment(profile)
    if args.prepare:
        prepare(args.prepare, env, args.evaluation_legacy_comments)
        return 0
    binary = env.get("NAGAMETV_TEST_BINARY")
    if binary and not args.evaluation_legacy_comments:
        return subprocess.run([binary, *arguments], cwd=ROOT, env=env,
                              pass_fds=lock_fds(), check=False).returncode
    with tempfile.TemporaryDirectory(prefix="nagametv-test-binary-") as directory:
        binary = Path(directory) / "nagametv"
        prepare(binary, env, args.evaluation_legacy_comments)
        return subprocess.run([str(binary), *arguments], cwd=ROOT, env=env,
                              pass_fds=lock_fds(), check=False).returncode


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, RuntimeError, subprocess.SubprocessError) as error:
        print(f"Qt test build/run failed: {error}", file=sys.stderr)
        sys.exit(1)
