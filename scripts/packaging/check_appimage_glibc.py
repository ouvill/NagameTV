#!/usr/bin/env python3
"""Inspect Linux ELF requirements without loading code or accessing hardware."""
import argparse
import os
from pathlib import Path
import re
import subprocess
import sys


def version(value):
    if not re.fullmatch(r"[0-9]+(?:\.[0-9]+)+", value):
        raise argparse.ArgumentTypeError(f"Invalid glibc version: {value}")
    return tuple(map(int, value.split(".")))


def check(app_dir, maximum):
    count = 0
    highest = (0,)
    failures = []
    for path in sorted(app_dir.rglob("*")):
        if path.is_symlink() or not path.is_file():
            continue
        with path.open("rb") as source:
            if source.read(4) != b"\x7fELF":
                continue
        count += 1
        result = subprocess.run(
            ["readelf", "--version-info", "--wide", str(path)],
            env=dict(os.environ, LC_ALL="C"), text=True, capture_output=True, check=True,
        )
        # Definitions describe exports, not requirements. In particular a
        # bundled libc must not make a newer dependency look compatible.
        needs = result.stdout.partition("Version needs section")[2]
        names = set(re.findall(r"Name: (GLIBC_\S+)", needs))
        for name in sorted(names):
            suffix = name.removeprefix("GLIBC_")
            if not re.fullmatch(r"[0-9]+(?:\.[0-9]+)+", suffix):
                failures.append(f"{path.relative_to(app_dir)}: unsupported requirement {name}")
                continue
            required = version(suffix)
            highest = max(highest, required)
            if required > maximum:
                failures.append(f"{path.relative_to(app_dir)}: requires {name}")
    if not count:
        raise ValueError(f"No ELF files found in {app_dir}")
    if failures:
        raise ValueError("glibc baseline exceeded:\n" + "\n".join(failures))
    print(f"AppImage: checked {count} ELF files; highest glibc requirement: {'.'.join(map(str, highest))}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("app_dir", type=Path)
    parser.add_argument("--max-glibc", required=True, type=version)
    args = parser.parse_args()
    try:
        check(args.app_dir, args.max_glibc)
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f"AppImage: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
