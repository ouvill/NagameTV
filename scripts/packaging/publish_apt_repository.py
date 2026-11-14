#!/usr/bin/env python3
"""Publish signed apt indexes for a published version release (writes to GitHub).

For each Ubuntu release, the indexes are attached to the GitHub Release
`apt-<codename>` and point at the deb of the version release. All checks and
signing finish before the first GitHub write.
"""

import argparse
from dataclasses import dataclass
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import subprocess

from scripts.packaging.apt_repository import VerifiedDeb, build, sha256, suite
from scripts.packaging.metadata import UbuntuRelease, deb_filename, validate_version

INDEX_FILES = ("Packages", "Release", "InRelease")


class PublishError(Exception):
    pass


def gh(*args: str, capture: bool = True) -> str:
    result = subprocess.run(["gh", *args], check=True, text=True,
                            capture_output=capture)
    return result.stdout if capture else ""


@dataclass(frozen=True)
class GitHubRelease:
    tag: str
    draft: bool
    prerelease: bool
    digests: dict[str, str]


def releases(repo: str) -> dict[str, GitHubRelease]:
    # Listing distinguishes 'no release' from API or authentication failures.
    pages = json.loads(gh("api", "--paginate", "--slurp", f"repos/{repo}/releases?per_page=100"))
    found = {}
    for page in pages:
        for item in page:
            found[item["tag_name"]] = GitHubRelease(
                item["tag_name"], item["draft"], item["prerelease"],
                {asset["name"]: asset.get("digest") or "" for asset in item["assets"]})
    return found


def published_version(apt_release: GitHubRelease) -> str | None:
    if "Packages" not in apt_release.digests:
        return None
    packages = gh("release", "download", apt_release.tag, "--pattern", "Packages", "--output", "-")
    versions = [line.removeprefix("Version:").strip() for line in packages.splitlines()
                if line.startswith("Version:")]
    if len(versions) != 1:
        raise PublishError(f"{apt_release.tag}: expected one package in Packages, found {len(versions)}")
    return versions[0]


def version_less_than(left: str, right: str) -> bool:
    return subprocess.run(["dpkg", "--compare-versions", left, "lt", right]).returncode == 0


@dataclass(frozen=True)
class PreparedSuite:
    """Signed indexes for one Ubuntu release, checked against GitHub's state."""

    ubuntu: UbuntuRelease
    directory: Path
    existing: GitHubRelease | None


def prepare(tag: str, asset_dir: Path, work: Path, signing_key: str, keyring: Path,
            repo: str) -> list[PreparedSuite]:
    if not tag.startswith("v"):
        raise PublishError(f"Expected a version tag such as v0.8.0, got {tag}")
    version = validate_version(tag[1:])
    if "-" in version.partition("+")[0]:
        raise PublishError(f"Prerelease {tag} is not published to apt")
    known = releases(repo)
    release = known.get(tag)
    if release is None or release.draft or release.prerelease:
        raise PublishError(f"{tag} must be a published, non-prerelease GitHub Release")
    date = datetime.now(timezone.utc)
    prepared = []
    for ubuntu in UbuntuRelease:
        name = deb_filename(version, ubuntu)
        deb = VerifiedDeb.open(asset_dir / name, version, ubuntu)
        # Sign exactly the bytes users download from the release.
        if release.digests.get(name) != f"sha256:{sha256(deb.path)}":
            raise PublishError(f"{name} does not match the digest of the {tag} release asset")
        existing = known.get(suite(ubuntu))
        if existing is not None:
            if existing.draft or existing.prerelease:
                raise PublishError(f"{existing.tag} must be a published release")
            current = published_version(existing)
            if current is not None and version_less_than(deb.debian_version, current):
                raise PublishError(f"{existing.tag} already serves {current}; "
                                   f"refusing to downgrade to {deb.debian_version}")
        directory = work / ubuntu.codename
        build(deb, directory, signing_key, keyring, date)
        prepared.append(PreparedSuite(ubuntu, directory, existing))
    return prepared


def publish(prepared: list[PreparedSuite], tag: str, repo: str) -> None:
    commit = gh("api", f"repos/{repo}/commits/{tag}", "--jq", ".sha").strip()
    for item in prepared:
        files = [str(item.directory / name) for name in INDEX_FILES]
        name = suite(item.ubuntu)
        if item.existing is None:
            # Never Latest: AppImage updates and /releases/latest follow the app release.
            gh("release", "create", name, *files, "--target", commit, "--latest=false",
               "--title", f"apt repository: Ubuntu {item.ubuntu.value}",
               "--notes", "Signed apt indexes for NagameTV. See docs/apt-repository.md.", capture=False)
            continue
        # One file per call keeps the order: InRelease, which apt reads first,
        # changes only after the files it lists.
        for path in files:
            gh("release", "upload", name, path, "--clobber", capture=False)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("tag", help="published version tag, for example v0.8.0")
    parser.add_argument("asset_dir", type=Path, help="directory containing the release debs")
    parser.add_argument("work", type=Path, help="new directory for the generated indexes")
    parser.add_argument("--signing-key", required=True, help="fingerprint of the signing subkey in GNUPGHOME")
    parser.add_argument("--keyring", type=Path, required=True, help="public keyring shipped to users")
    args = parser.parse_args()
    repo = os.environ.get("GH_REPO")
    if not repo:
        parser.exit(2, "Set GH_REPO to owner/repository\n")
    try:
        prepared = prepare(args.tag, args.asset_dir, args.work, args.signing_key, args.keyring, repo)
        publish(prepared, args.tag, repo)
    except (PublishError, ValueError, OSError) as error:
        parser.exit(1, f"apt publish: {error}\n")
    except subprocess.CalledProcessError as error:
        detail = f"\n{error.stderr}" if error.stderr else ""
        parser.exit(1, f"apt publish: {error}{detail}\n")


if __name__ == "__main__":
    main()
