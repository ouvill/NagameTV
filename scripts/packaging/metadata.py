#!/usr/bin/env python3
"""Shared release filenames and Ubuntu deb versions (no hardware access)."""

import argparse
from enum import Enum
import platform
import re


class Architecture(str, Enum):
    X86_64 = "x86_64"
    AARCH64 = "aarch64"

    @property
    def debian(self) -> str:
        match self:
            case Architecture.X86_64:
                return "amd64"
            case Architecture.AARCH64:
                return "arm64"

    @property
    def elf_machine(self) -> int:
        match self:
            case Architecture.X86_64:
                return 62  # ELF EM_X86_64
            case Architecture.AARCH64:
                return 183  # ELF EM_AARCH64

    @classmethod
    def native(cls) -> "Architecture":
        if platform.system() != "Linux":
            raise ValueError("Only native Linux builds are supported")
        return cls(platform.machine())


class UbuntuRelease(str, Enum):
    NOBLE = "24.04"
    RESOLUTE = "26.04"

    @property
    def codename(self) -> str:
        match self:
            case UbuntuRelease.NOBLE:
                return "noble"
            case UbuntuRelease.RESOLUTE:
                return "resolute"


def deb_architectures(ubuntu: UbuntuRelease) -> tuple[Architecture, ...]:
    match ubuntu:
        case UbuntuRelease.NOBLE:
            # The bundled Qt 6.8.3 ARM SDK lacks the required WebEngine module.
            return (Architecture.X86_64,)
        case UbuntuRelease.RESOLUTE:
            return (Architecture.X86_64, Architecture.AARCH64)


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


def deb_filename(version: str, ubuntu: UbuntuRelease,
                 architecture: Architecture = Architecture.X86_64) -> str:
    if architecture not in deb_architectures(ubuntu):
        raise ValueError(f"Ubuntu {ubuntu.value} deb does not support {architecture.value}; use Flatpak")
    return f"nagametv_{debian_version(version, ubuntu)}_{architecture.debian}.deb"


def appimage_filename(version: str) -> str:
    validate_version(version)
    return f"nagametv-{version}-x86_64.AppImage"


def release_assets(version: str) -> list[str]:
    validate_version(version)
    return [appimage_filename(version)] + [
        f"nagametv-{version}-{architecture.value}.flatpak" for architecture in Architecture
    ] + [
        deb_filename(version, ubuntu, architecture)
        for ubuntu in UbuntuRelease for architecture in deb_architectures(ubuntu)
    ]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("version")
    parser.add_argument("--appimage", action="store_true", help="print only the AppImage filename")
    args = parser.parse_args()
    try:
        print(appimage_filename(args.version) if args.appimage else "\n".join(release_assets(args.version)))
    except ValueError as error:
        parser.exit(1, f"Package metadata: {error}\n")


if __name__ == "__main__":
    main()
