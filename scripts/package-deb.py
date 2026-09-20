#!/usr/bin/env python3
"""Finish an Ubuntu amd64 package staged by build-deb.sh, without hardware access."""

import argparse
import hashlib
import os
from pathlib import Path
import re
import subprocess
import tempfile
import tomllib

from package_metadata import UbuntuRelease, deb_filename, debian_version


ROOT = Path(__file__).resolve().parent.parent
PACKAGE = "nagametv"


def elf_files(stage: Path) -> list[Path]:
    result = []
    for path in sorted(stage.rglob("*")):
        if path.is_file() and not path.is_symlink():
            with path.open("rb") as stream:
                if stream.read(4) == b"\x7fELF":
                    result.append(path)
    if not result:
        raise ValueError("No ELF files in package")
    return result


def shared_dependencies(stage: Path, ubuntu: UbuntuRelease) -> str:
    binaries = elf_files(stage)
    with tempfile.TemporaryDirectory(prefix="shlibdeps-", dir=stage.parent) as work:
        debian = Path(work) / "debian"
        debian.mkdir()
        (debian / "control").write_text(
            f"Source: {PACKAGE}\n\nPackage: {PACKAGE}\nArchitecture: amd64\n")
        options = []
        match ubuntu:
            case UbuntuRelease.NOBLE:
                # Declare only libraries actually shipped in the private bundle as
                # self-dependencies. Missing system libraries must still fail.
                shlibs = set()
                for binary in binaries:
                    dynamic = subprocess.check_output(["readelf", "-d", binary], text=True)
                    soname = re.search(r"\(SONAME\).*\[(.+)\]", dynamic)
                    if soname:
                        # Match both dpkg SONAME forms, including PulseAudio's
                        # private libpulsecommon-<version>.so.
                        name = (re.fullmatch(r"(.+)\.so\.(.+)", soname[1])
                                or re.fullmatch(r"(.+)-(\d.*)\.so", soname[1]))
                        if not name:
                            # GStreamer plugins have unversioned SONAMEs (e.g.
                            # libgstapp.so). dpkg treats these as private modules;
                            # scan their dependencies, but do not invent an ABI.
                            if soname[1].endswith(".so"):
                                continue
                            raise ValueError(f"Unsupported bundled SONAME: {soname[1]}")
                        shlibs.add(f"{name[1]} {name[2]} {PACKAGE}\n")
                (debian / "shlibs.local").write_text("".join(sorted(shlibs)))
                options = [f"-l{stage}/opt/nagametv/usr/lib", f"-x{PACKAGE}"]
            case UbuntuRelease.RESOLUTE:
                pass
        output = subprocess.check_output(
            ["dpkg-shlibdeps", "-O", "--warnings=1", *options, *[f"-e{path}" for path in binaries]],
            cwd=work, text=True,
        )
        return next(line.removeprefix("shlibs:Depends=") for line in output.splitlines()
                    if line.startswith("shlibs:Depends="))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("ubuntu", type=UbuntuRelease, choices=list(UbuntuRelease))
    parser.add_argument("stage", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    stage = args.stage.resolve()
    version = tomllib.loads((ROOT / "rust/Cargo.toml").read_text())["package"]["version"]
    try:
        # dpkg-shlibdeps uses this directory to identify the package root and
        # resolve $ORIGIN paths relative to the staged files.
        control = stage / "DEBIAN"
        control.mkdir()
        dependencies = [shared_dependencies(stage, args.ubuntu)]
        match args.ubuntu:
            case UbuntuRelease.NOBLE:
                dependencies += ["ca-certificates", "fonts-noto-cjk"]
            case UbuntuRelease.RESOLUTE:
                dependencies += [line for line in (ROOT / "packaging/deb/runtime-dependencies-26.04.txt")
                                 .read_text().splitlines() if line and not line.startswith("#")]
        installed_size = subprocess.check_output(["du", "-sk", stage], text=True).split()[0]
        (control / "control").write_text(
            f"Package: {PACKAGE}\nVersion: {debian_version(version, args.ubuntu)}\n"
            "Section: video\nPriority: optional\nArchitecture: amd64\n"
            "Maintainer: ouvill <contact@ouvill.net>\n"
            "Homepage: https://github.com/ouvill/NagameTV\n"
            f"Installed-Size: {installed_size}\nDepends: {', '.join(dependencies)}\n"
            "Recommends: xdg-desktop-portal\n"
            "Description: Television viewer with subtitles and live comments\n"
            f" Mirakurun live TV and local TS recordings, packaged for Ubuntu {args.ubuntu.value}.\n"
        )
        with (control / "md5sums").open("w") as sums:
            for path in sorted(stage.rglob("*")):
                if path.is_file() and not path.is_symlink() and control not in path.parents:
                    with path.open("rb") as stream:
                        checksum = hashlib.file_digest(stream, "md5").hexdigest()
                    sums.write(f"{checksum}  {path.relative_to(stage)}\n")
        # Publish only a complete package, retaining the previous build on failure.
        filename = deb_filename(version, args.ubuntu)
        pending = stage.parent / filename
        subprocess.run(["dpkg-deb", "--root-owner-group", "--build", stage, pending], check=True)
        with pending.open("rb") as stream:
            checksum = hashlib.file_digest(stream, "sha256").hexdigest()
        args.output.mkdir(parents=True, exist_ok=True)
        os.replace(pending, args.output / filename)
        (args.output / f"{filename}.sha256").write_text(f"{checksum}  {filename}\n")
        print(f"Package: {args.output / filename}")
    except subprocess.CalledProcessError as error:
        parser.exit(1, f"deb: {error.cmd[0]} failed (exit {error.returncode})\n")
    except (ValueError, OSError) as error:
        parser.exit(1, f"deb: {error}\n")


if __name__ == "__main__":
    main()
