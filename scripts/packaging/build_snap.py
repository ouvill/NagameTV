#!/usr/bin/env python3
"""Build a native NagameTV snap and checksum without accessing devices."""

import argparse
import hashlib
from pathlib import Path
import shutil
import subprocess
import tempfile
import tomllib

from scripts.packaging.metadata import Architecture, validate_version


def build(project: Path, *, destructive_mode: bool) -> Path:
    architecture = Architecture.native().debian
    with (project / "rust/Cargo.toml").open("rb") as manifest:
        version = validate_version(tomllib.load(manifest)["package"]["version"])
    output = project / "build/snap"
    output.mkdir(parents=True, exist_ok=True)
    filename = f"nagametv_{version}_{architecture}.snap"
    snapcraft = shutil.which("snapcraft") or "/snap/bin/snapcraft"
    mode = "--destructive-mode" if destructive_mode else "--use-lxd"

    with tempfile.TemporaryDirectory(prefix=".packing-", dir=output) as temporary:
        subprocess.run(
            [snapcraft, "pack", mode, "--build-for", architecture, "--output", temporary],
            cwd=project,
            check=True,
        )
        snap = Path(temporary) / filename
        with snap.open("rb") as artifact:
            checksum = hashlib.file_digest(artifact, "sha256").hexdigest()
        checksum_file = snap.with_suffix(".snap.sha256")
        checksum_file.write_text(f"{checksum}  {filename}\n")
        shutil.move(snap, output / snap.name)
        shutil.move(checksum_file, output / checksum_file.name)
    return output / filename


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--destructive-mode",
        action="store_true",
        help="run as root in a disposable Ubuntu 26.04 build environment",
    )
    args = parser.parse_args()

    try:
        print(build(Path(__file__).resolve().parents[2], destructive_mode=args.destructive_mode))
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"Snap build: {error}\n")


if __name__ == "__main__":
    main()
