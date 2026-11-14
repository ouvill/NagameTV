#!/usr/bin/env python3
"""Build signed flat apt repository indexes for one Ubuntu release (no hardware access).

The output directory holds Packages, Release and InRelease. They are published
as the assets of the GitHub Release `apt-<codename>`, and apt reads them as a
flat repository whose URI is the release download root:

    URIs: https://github.com/OWNER/REPO/releases/download/
    Suites: apt-noble/

Packages refers to the deb already attached to the version release
(`v0.8.0/nagametv_...deb`), so the large deb is not uploaded twice.
"""

import argparse
from dataclasses import dataclass
from datetime import datetime, timezone
import email.utils
import hashlib
import os
from pathlib import Path
import subprocess
import tempfile

from scripts.packaging.metadata import UbuntuRelease, deb_filename, debian_version

PACKAGE = "nagametv"
ARCHITECTURE = "amd64"
ORIGIN = "NagameTV"
CHUNK_BYTES = 1 << 20


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        while chunk := stream.read(CHUNK_BYTES):
            digest.update(chunk)
    return digest.hexdigest()


@dataclass(frozen=True)
class VerifiedDeb:
    """A deb whose control fields match the release it is published for."""

    path: Path
    control: str
    version: str
    ubuntu: UbuntuRelease

    @classmethod
    def open(cls, path: Path, version: str, ubuntu: UbuntuRelease) -> "VerifiedDeb":
        expected_name = deb_filename(version, ubuntu)
        if path.name != expected_name:
            raise ValueError(f"Expected {expected_name}, got {path.name}")
        control = subprocess.run(["dpkg-deb", "--field", str(path)], check=True,
                                 capture_output=True, text=True).stdout.rstrip("\n")
        fields = {}
        for line in control.splitlines():
            if line and not line[0].isspace():
                name, _, value = line.partition(":")
                fields[name] = value.strip()
        expected = {"Package": PACKAGE, "Architecture": ARCHITECTURE,
                    "Version": debian_version(version, ubuntu)}
        for name, value in expected.items():
            if fields.get(name) != value:
                raise ValueError(f"{path.name}: {name} is {fields.get(name)!r}, expected {value!r}")
        for name in ("Filename", "Size", "SHA256", "MD5sum"):
            if name in fields:
                raise ValueError(f"{path.name}: control must not define index field {name}")
        return cls(path, control, version, ubuntu)

    @property
    def debian_version(self) -> str:
        return debian_version(self.version, self.ubuntu)

    def stanza(self) -> str:
        # Relative to the release download root: the version release's asset.
        return (f"{self.control}\nFilename: v{self.version}/{self.path.name}\n"
                f"Size: {self.path.stat().st_size}\nSHA256: {sha256(self.path)}\n")


def suite(ubuntu: UbuntuRelease) -> str:
    return f"apt-{ubuntu.codename}"


def release_text(packages: Path, ubuntu: UbuntuRelease, date: datetime) -> str:
    # No Valid-Until: releases are irregular, and an expired index would break
    # `apt update` the same way an expired key would. Suite must equal the
    # flat-repository path users configure, or apt warns about a conflict.
    return (f"Origin: {ORIGIN}\nLabel: {ORIGIN}\nSuite: {suite(ubuntu)}\n"
            f"Architectures: {ARCHITECTURE}\n"
            f"Date: {email.utils.format_datetime(date.astimezone(timezone.utc), usegmt=True)}\n"
            f"Description: NagameTV for Ubuntu {ubuntu.value}\n"
            f"SHA256:\n {sha256(packages)} {packages.stat().st_size} {packages.name}\n")


def build(deb: VerifiedDeb, output: Path, signing_key: str, keyring: Path, date: datetime) -> None:
    output.mkdir(parents=True, exist_ok=False)
    packages = output / "Packages"
    packages.write_text(deb.stanza())
    release = output / "Release"
    release.write_text(release_text(packages, deb.ubuntu, date))
    in_release = output / "InRelease"
    # '!' pins the exact (sub)key instead of letting gpg pick a newer one.
    subprocess.run(["gpg", "--batch", "--yes", "--no-tty", "--local-user", f"{signing_key}!",
                    "--digest-algo", "SHA512", "--clearsign", "--output", str(in_release), str(release)],
                   check=True)
    verify(in_release, keyring)


def verify(in_release: Path, keyring: Path) -> None:
    # apt trusts only the keyring shipped to users; signing with any other key
    # must fail here instead of after upload.
    with tempfile.TemporaryDirectory(prefix="apt-verify-") as home:
        subprocess.run(["gpgv", "--homedir", home, "--keyring", str(keyring.resolve()), str(in_release)],
                       check=True, capture_output=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("ubuntu", type=UbuntuRelease, choices=list(UbuntuRelease))
    parser.add_argument("version", help="application version, for example 0.8.0")
    parser.add_argument("asset_dir", type=Path, help="directory containing the release debs")
    parser.add_argument("output", type=Path, help="new directory for Packages, Release and InRelease")
    parser.add_argument("--signing-key", required=True, help="fingerprint of the signing (sub)key in GNUPGHOME")
    parser.add_argument("--keyring", type=Path, required=True, help="public keyring shipped to users")
    args = parser.parse_args()
    try:
        deb = VerifiedDeb.open(args.asset_dir / deb_filename(args.version, args.ubuntu), args.version, args.ubuntu)
        epoch = os.environ.get("SOURCE_DATE_EPOCH")
        date = datetime.fromtimestamp(int(epoch), timezone.utc) if epoch else datetime.now(timezone.utc)
        build(deb, args.output, args.signing_key, args.keyring, date)
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        stderr = getattr(error, "stderr", None)
        detail = f"\n{stderr.decode(errors='replace') if isinstance(stderr, bytes) else stderr}" if stderr else ""
        parser.exit(1, f"apt repository: {error}{detail}\n")


if __name__ == "__main__":
    main()
