#!/usr/bin/env python3
"""Shared release filenames and Ubuntu deb versions (no hardware access)."""

import argparse
from enum import Enum
import re


class UbuntuRelease(str, Enum):
    NOBLE = "24.04"
    RESOLUTE = "26.04"


def validate_version(version: str) -> str:
    # Cargo/SemVer, including prereleases; prevent path/output injection.
    number = r"(?:0|[1-9][0-9]*)"
    identifier = r"(?:0|[1-9][0-9]*|[0-9]*[A-Za-z-][0-9A-Za-z-]*)"
    semver = rf"{number}\.{number}\.{number}(?:-{identifier}(?:\.{identifier})*)?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?"
    if not re.fullmatch(semver, version):
        raise ValueError(f"Invalid package version: {version!r}")
    return version


def debian_version(version: str, ubuntu: UbuntuRelease) -> str:
    validate_version(version)
    upstream, plus, metadata = version.partition("+")
    # Debian sorts '~rc.1' before the final release; '-' would sort after it.
    upstream = upstream.replace("-", "~", 1)
    return f"{upstream}{plus}{metadata}-1ubuntu{ubuntu.value}"


def deb_filename(version: str, ubuntu: UbuntuRelease) -> str:
    return f"nagametv_{debian_version(version, ubuntu)}_amd64.deb"


def release_assets(version: str) -> list[str]:
    validate_version(version)
    return [f"nagametv-{version}-x86_64.{extension}" for extension in ("AppImage", "flatpak")] + [
        deb_filename(version, ubuntu) for ubuntu in UbuntuRelease
    ]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("version")
    args = parser.parse_args()
    try:
        print("\n".join(release_assets(args.version)))
    except ValueError as error:
        parser.exit(1, f"Package metadata: {error}\n")


if __name__ == "__main__":
    main()
