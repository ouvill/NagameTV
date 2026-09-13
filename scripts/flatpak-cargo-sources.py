#!/usr/bin/env python3
"""Generate offline Flatpak sources from this application's crates.io lockfile."""
import argparse
import json
from pathlib import Path
import re
import tomllib


def generate(lockfile):
    sources = []
    for package in tomllib.loads(lockfile.read_text())["package"]:
        registry = package.get("source")
        if registry is None:
            continue  # Local crates and qt-build-utils are copied with the project.
        if registry != "registry+https://github.com/rust-lang/crates.io-index":
            raise ValueError(f"Unsupported source for {package['name']}: {registry}")
        checksum = package.get("checksum", "")
        if not re.fullmatch(r"[0-9a-f]{64}", checksum):
            raise ValueError(f"Missing or invalid checksum for {package['name']}")
        name, version = package["name"], package["version"]
        destination = f"cargo/vendor/{name}-{version}"
        sources.extend([
            {
                "type": "archive",
                "archive-type": "tar-gzip",
                "url": f"https://static.crates.io/crates/{name}/{name}-{version}.crate",
                "sha256": checksum,
                "dest": destination,
            },
            {
                "type": "inline",
                "contents": json.dumps({"package": checksum, "files": {}}),
                "dest": destination,
                "dest-filename": ".cargo-checksum.json",
            },
        ])
    sources.append({
        "type": "inline",
        "contents": '[source.crates-io]\nreplace-with = "vendored-sources"\n'
                    '[source.vendored-sources]\ndirectory = "cargo/vendor"\n'
                    '[net]\noffline = true\n',
        "dest": "cargo",
        "dest-filename": "config.toml",
    })
    return json.dumps(sources, indent=2) + "\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="Check that the committed sources match Cargo.lock")
    args = parser.parse_args()
    project = Path(__file__).resolve().parent.parent
    output = project / "packaging/flatpak/cargo-sources.json"
    contents = generate(project / "rust/Cargo.lock")
    if args.check:
        if not output.is_file() or output.read_text() != contents:
            parser.exit(1, "Flatpak Cargo sources are stale; run scripts/flatpak-cargo-sources.py\n")
    else:
        output.write_text(contents)


if __name__ == "__main__":
    main()
