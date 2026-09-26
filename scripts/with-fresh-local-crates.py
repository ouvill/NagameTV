#!/usr/bin/env python3
"""Keep restored dependencies, but rebuild repository code before running CI."""
import argparse
import json
import os
import subprocess

from test_support import ROOT, ensure_lock, lock_fds


def clean_local_packages(manifest, env):
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--manifest-path", str(manifest), "--locked",
         "--format-version", "1"], env=env, text=True, pass_fds=lock_fds()))
    # Include path dependencies and local patches, not just workspace members.
    packages = [package for package in metadata["packages"] if package["source"] is None]
    if not packages:
        raise RuntimeError("No local Cargo packages found; refusing an unscoped cargo clean")
    command = ["cargo", "clean", "--manifest-path", str(manifest), "--locked"]
    for package in packages:
        command.extend(["--package", package["id"]])
    print("Rebuilding local Cargo packages: " + ", ".join(p["name"] for p in packages), flush=True)
    subprocess.run(command, env=env, check=True, pass_fds=lock_fds())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    if not args.command:
        parser.error("a command to run after cleaning is required")
    ensure_lock()
    clean_local_packages(ROOT / "rust/Cargo.toml", os.environ)
    os.execvpe(args.command[0], args.command, os.environ)


if __name__ == "__main__":
    main()
