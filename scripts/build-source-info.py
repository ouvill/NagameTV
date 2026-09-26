#!/usr/bin/env python3
"""Carry source identity across Linux Docker/Flatpak packaging boundaries."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess


def source_fingerprint(root):
    """Hash Git-visible inputs, including deletions, symlinks and submodules.

    Do not use mtimes: checkout and restoring a cache can preserve or change
    them independently of file contents. Ignored build outputs never enter the
    digest. Only the digest is exported, never file contents or Git diffs.
    """
    digest = hashlib.sha256()

    def add(value):
        digest.update(len(value).to_bytes(8, "big"))
        digest.update(value)

    def visit(directory):
        entries = subprocess.check_output(
            ["git", "--no-optional-locks", "-C", str(directory), "ls-files", "-z",
             "--cached", "--others", "--exclude-standard"]).split(b"\0")
        for name in sorted(set(entries) - {b""}):
            path = directory / os.fsdecode(name)
            add(os.fsencode(path.relative_to(root)))
            if path.is_symlink():
                add(b"symlink")
                add(os.fsencode(os.readlink(path)))
            elif path.is_file():
                add(b"file")
                add(str(path.stat().st_mode).encode())
                with path.open("rb") as source:
                    add(hashlib.file_digest(source, "sha256").digest())
            elif path.is_dir():
                # Git tracks directories only as gitlinks. Untracked file names
                # from ls-files are already expanded, not directory entries.
                add(b"submodule")
                if (path / ".git").exists():
                    add(subprocess.check_output(
                        ["git", "-C", str(path), "rev-parse", "--verify", "HEAD"]))
                    visit(path)
                else:
                    add(b"uninitialized")
            else:
                add(b"deleted")

    visit(root)
    return digest.hexdigest()


def source_identity(root, *, env=None):
    override = (os.environ if env is None else env).get("NAGAMETV_BUILD_SOURCE")
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
    parser.add_argument("--cargo-env", action="store_true",
                        help="emit source root and input digest for the build lock wrapper")
    args = parser.parse_args()
    if bool(args.flatpak_manifest) != bool(args.output):
        parser.error("--flatpak-manifest and --output must be provided together")
    root = Path(__file__).resolve().parent.parent
    if args.cargo_env:
        if args.flatpak_manifest:
            parser.error("--cargo-env cannot be combined with --flatpak-manifest")
        # Archives have no reliable list of ignored outputs. Keep the collector's
        # conservative per-invocation refresh for that case.
        if (root / ".git").exists():
            # Include HEAD and clean/dirty even when the file contents are the
            # same (for example an empty commit or staging a reverted edit).
            inputs = source_identity(root) + ":" + source_fingerprint(root)
            print(root)
            print(hashlib.sha256(inputs.encode()).hexdigest())
        return
    identity = source_identity(root)
    if args.flatpak_manifest:
        manifest = flatpak_manifest(args.flatpak_manifest.resolve(), identity)
        args.output.write_text(json.dumps(manifest, indent=2) + "\n")
    else:
        print(identity)


if __name__ == "__main__":
    main()
