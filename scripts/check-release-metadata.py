#!/usr/bin/env python3
"""Check package metadata before CI builds or creates a tagged release."""

import argparse
import json
from pathlib import Path
import subprocess
import tomllib
import xml.etree.ElementTree as ET

from package_metadata import validate_version


def validate(project: Path, tag: str | None) -> str:
    package = tomllib.loads((project / "rust/Cargo.toml").read_text())["package"]
    version = validate_version(package["version"])
    if tag is not None and tag != f"v{version}":
        raise ValueError(f"Tag {tag!r} must match Cargo.toml: v{version}")

    locked = tomllib.loads((project / "rust/Cargo.lock").read_text())["package"]
    app = [entry for entry in locked if entry["name"] == package["name"] and "source" not in entry]
    if len(app) != 1 or app[0]["version"] != version:
        raise ValueError("Application version in Cargo.lock does not match Cargo.toml")
    metadata = ET.parse(project / "packaging/linux/io.github.ouvill.nagametv.metainfo.xml")
    latest = metadata.find("releases/release")
    if latest is None or latest.get("version") != version:
        raise ValueError("First AppStream release version must match Cargo.toml")

    manifest = json.loads((project / "packaging/flatpak/io.github.ouvill.nagametv.json").read_text())
    sources = [source for module in manifest["modules"] for source in module.get("sources", [])
               if isinstance(source, dict) and source.get("type") == "git"]
    for path in ("third_party/libaribcaption", "third_party/tsreadex"):
        # Compare with the gitlink, not a possibly stale submodule work tree.
        gitlink = subprocess.check_output(
            ["git", "ls-files", "--stage", "--", path], cwd=project, text=True
        ).split()
        if len(gitlink) != 4 or gitlink[0] != "160000" or gitlink[2] != "0":
            raise ValueError(f"Missing or conflicted submodule gitlink: {path}")
        matching = [source for source in sources if source.get("dest") == path]
        if len(matching) != 1 or matching[0].get("commit") != gitlink[1]:
            raise ValueError(f"Flatpak source commit differs from submodule: {path}")
    return version


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tag", help="Require this tag to equal v<Cargo.toml version>")
    parser.add_argument("--github-output", type=Path, help="Append validated GitHub Actions outputs")
    args = parser.parse_args()
    try:
        version = validate(Path(__file__).resolve().parent.parent, args.tag)
    except (ValueError, KeyError, OSError, ET.ParseError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"Release metadata: {error}\n")
    if args.github_output is not None:
        with args.github_output.open("a") as output:
            output.write(f"version={version}\n")
    print(f"Release metadata OK: {version}")


if __name__ == "__main__":
    main()
