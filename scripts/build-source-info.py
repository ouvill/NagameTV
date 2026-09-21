#!/usr/bin/env python3
"""Carry source identity across Linux Docker/Flatpak packaging boundaries."""
import argparse
import json
import os
from pathlib import Path
import re
import subprocess


def source_identity(root):
    override = os.environ.get("NAGAMETV_BUILD_SOURCE")
    if override is not None:
        if override != "unavailable" and not re.fullmatch(r"(?:[0-9a-fA-F]{40}|[0-9a-fA-F]{64}):(clean|dirty)", override):
            raise ValueError("NAGAMETV_BUILD_SOURCE must be a full COMMIT:clean|dirty or unavailable")
        return override
    if not (root / ".git").exists():
        return "unavailable"

    def git(*args):
        return subprocess.check_output(
            ["git", "--no-optional-locks", "-C", str(root), *args], text=True).strip()

    commit = git("rev-parse", "--verify", "HEAD")
    status = git("status", "--porcelain", "--untracked-files=normal", "--ignore-submodules=none")
    return f"{commit}:{'dirty' if status else 'clean'}"


def flatpak_manifest(path, identity):
    """Rebase local inputs so the generated manifest can live under build/."""
    manifest = json.loads(path.read_text())
    for module in manifest["modules"]:
        sources = module["sources"]
        for index, source in enumerate(sources):
            if isinstance(source, str):
                sources[index] = str((path.parent / source).resolve())
            elif "path" in source:
                source["path"] = str((path.parent / source["path"]).resolve())
        if module["name"] == "nagametv":
            module["build-options"]["env"]["NAGAMETV_BUILD_SOURCE"] = identity
    return manifest


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--flatpak-manifest", type=Path)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    if bool(args.flatpak_manifest) != bool(args.output):
        parser.error("--flatpak-manifest and --output must be provided together")
    identity = source_identity(Path(__file__).resolve().parent.parent)
    if args.flatpak_manifest:
        manifest = flatpak_manifest(args.flatpak_manifest.resolve(), identity)
        args.output.write_text(json.dumps(manifest, indent=2) + "\n")
    else:
        print(identity)


if __name__ == "__main__":
    main()
