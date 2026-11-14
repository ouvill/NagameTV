#!/usr/bin/env python3
"""Hardware-free apt repository checks; GitHub writes are captured by a local fixture."""

from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

from scripts.packaging.apt_repository import VerifiedDeb, build
from scripts.packaging.metadata import UbuntuRelease, deb_filename, debian_version

ROOT = Path(__file__).resolve().parents[2]
VERSION = "0.8.1"
PREVIOUS = "0.8.0"
REPO = "example/nagametv"
FIXED_DATE = datetime(2026, 11, 14, tzinfo=timezone.utc)

FAKE_GH = """#!/usr/bin/env python3
import json, os, sys
from pathlib import Path
args = sys.argv[1:]
with Path(os.environ['FAKE_GH_LOG']).open('a') as log:
    log.write(json.dumps(args) + '\\n')
state = json.loads(Path(os.environ['FAKE_GH_STATE']).read_text())
if args[:3] == ['api', '--paginate', '--slurp']:
    print(json.dumps([state['releases']]))
elif args[0] == 'api' and '/commits/' in args[1]:
    print(state['sha'])
elif args[:2] == ['release', 'download']:
    sys.stdout.write(state['packages'][args[2]])
"""


def make_deb(directory: Path, version: str, ubuntu: UbuntuRelease, package_version: str | None = None) -> Path:
    stage = directory / f"stage-{version}-{ubuntu.value}"
    (stage / "DEBIAN").mkdir(parents=True)
    (stage / "DEBIAN/control").write_text(
        f"Package: nagametv\nVersion: {package_version or debian_version(version, ubuntu)}\n"
        "Architecture: amd64\nMaintainer: Test <test@example.invalid>\n"
        "Description: test package\n second line\n")
    output = directory / deb_filename(version, ubuntu)
    subprocess.run(["dpkg-deb", "--root-owner-group", "--build", str(stage), str(output)],
                   check=True, capture_output=True)
    return output


class GpgHome:
    """A private keyring with a certify-only primary key and a signing subkey."""

    def __init__(self, directory: Path):
        self.home = directory / "gnupg"
        self.home.mkdir(mode=0o700)
        self.env = dict(os.environ, GNUPGHOME=str(self.home))
        self.signing_key = self.add_key("NagameTV test <test@example.invalid>")
        self.keyring = directory / "keyring.gpg"
        self.keyring.write_bytes(self.run("--export", self.primary))

    def run(self, *args: str) -> bytes:
        return subprocess.run(["gpg", "--batch", "--quiet", "--passphrase", "", *args], env=self.env,
                              check=True, capture_output=True).stdout

    def gpg(self, *args: str) -> str:
        return self.run(*args).decode()

    def fingerprints(self, user: str) -> list[str]:
        listing = self.gpg("--with-colons", "--list-keys", user)
        return [line.split(":")[9] for line in listing.splitlines() if line.startswith("fpr:")]

    def add_key(self, user: str) -> str:
        self.gpg("--quick-gen-key", user, "ed25519", "cert", "never")
        primary = self.fingerprints(user)[0]
        self.gpg("--quick-add-key", primary, "ed25519", "sign", "never")
        if not hasattr(self, "primary"):
            self.primary = primary
        return self.fingerprints(user)[1]

    def close(self):
        subprocess.run(["gpgconf", "--kill", "gpg-agent"], env=self.env, check=False)


class AptRepositoryTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.directory = Path(temporary.name)
        self.gpg = GpgHome(self.directory)
        self.addCleanup(self.gpg.close)
        self.assets = self.directory / "assets"
        self.assets.mkdir()

    def build(self, ubuntu=UbuntuRelease.NOBLE, signing_key=None) -> Path:
        deb = VerifiedDeb.open(make_deb(self.assets, VERSION, ubuntu), VERSION, ubuntu)
        output = self.directory / "repo"
        with mock.patch.dict(os.environ, GNUPGHOME=str(self.gpg.home)):
            build(deb, output, signing_key or self.gpg.signing_key, self.gpg.keyring, FIXED_DATE)
        return output

    def test_indexes_point_at_version_release_and_verify_with_shipped_keyring(self):
        output = self.build()
        packages = (output / "Packages").read_text()
        deb = self.assets / deb_filename(VERSION, UbuntuRelease.NOBLE)
        self.assertIn(f"Filename: v{VERSION}/{deb.name}\n", packages)
        self.assertIn(f"SHA256: {hashlib.sha256(deb.read_bytes()).hexdigest()}\n", packages)
        self.assertIn(" second line\n", packages)
        release = (output / "Release").read_text()
        self.assertIn("Suite: apt-noble\n", release)
        self.assertNotIn("Valid-Until", release)
        packages_hash = hashlib.sha256((output / "Packages").read_bytes()).hexdigest()
        self.assertIn(f" {packages_hash} {len((output / 'Packages').read_bytes())} Packages\n", release)
        subprocess.run(["gpgv", "--homedir", str(self.directory), "--keyring", str(self.gpg.keyring),
                        str(output / "InRelease")], check=True, capture_output=True)

    def test_key_outside_shipped_keyring_is_rejected_before_upload(self):
        other = self.gpg.add_key("Other <other@example.invalid>")
        with self.assertRaises(subprocess.CalledProcessError):
            self.build(signing_key=other)

    def test_deb_for_another_release_or_version_is_rejected(self):
        noble = make_deb(self.assets, VERSION, UbuntuRelease.NOBLE)
        with self.assertRaisesRegex(ValueError, "Expected"):
            VerifiedDeb.open(noble, VERSION, UbuntuRelease.RESOLUTE)
        wrong = self.directory / "wrong"
        wrong.mkdir()
        mismatched = make_deb(wrong, VERSION, UbuntuRelease.NOBLE, package_version="0.8.0-1ubuntu24.04")
        with self.assertRaisesRegex(ValueError, "Version"):
            VerifiedDeb.open(mismatched, VERSION, UbuntuRelease.NOBLE)


class PublishTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.directory = Path(temporary.name)
        self.gpg = GpgHome(self.directory)
        self.addCleanup(self.gpg.close)
        self.assets = self.directory / "assets"
        self.assets.mkdir()
        digests = {}
        for ubuntu in UbuntuRelease:
            deb = make_deb(self.assets, VERSION, ubuntu)
            digests[deb.name] = f"sha256:{hashlib.sha256(deb.read_bytes()).hexdigest()}"
        self.version_release = {"tag_name": f"v{VERSION}", "draft": False, "prerelease": False,
                                "assets": [{"name": name, "digest": digest} for name, digest in digests.items()]}
        self.state = {"releases": [self.version_release], "packages": {}, "sha": "a" * 40}
        bin_dir = self.directory / "bin"
        bin_dir.mkdir()
        (bin_dir / "gh").write_text(FAKE_GH)
        (bin_dir / "gh").chmod(0o755)
        self.log = self.directory / "gh.jsonl"
        self.state_file = self.directory / "state.json"
        self.env = dict(self.gpg.env, PATH=f"{bin_dir}:{os.environ['PATH']}", GH_REPO=REPO,
                        PYTHONPATH=str(ROOT), FAKE_GH_LOG=str(self.log), FAKE_GH_STATE=str(self.state_file))

    def add_apt_release(self, ubuntu: UbuntuRelease, version: str):
        tag = f"apt-{ubuntu.codename}"
        self.state["releases"].append({"tag_name": tag, "draft": False, "prerelease": False,
                                       "assets": [{"name": name, "digest": "sha256:0"}
                                                  for name in ("Packages", "Release", "InRelease")]})
        self.state["packages"][tag] = f"Package: nagametv\nVersion: {debian_version(version, ubuntu)}\n"

    def publish(self, tag=f"v{VERSION}"):
        self.state_file.write_text(json.dumps(self.state))
        return subprocess.run(
            [sys.executable, "-m", "scripts.packaging.publish_apt_repository", tag, str(self.assets),
             str(self.directory / "work"), "--signing-key", self.gpg.signing_key, "--keyring", str(self.gpg.keyring)],
            cwd=ROOT, env=self.env, text=True, capture_output=True)

    def writes(self):
        if not self.log.exists():
            return []
        calls = [json.loads(line) for line in self.log.read_text().splitlines()]
        return [call for call in calls if call[:2] in (["release", "create"], ["release", "upload"])]

    def test_first_publish_creates_releases_that_never_become_latest(self):
        result = self.publish()
        self.assertEqual(result.returncode, 0, result.stderr)
        writes = self.writes()
        self.assertEqual([call[2] for call in writes], ["apt-noble", "apt-resolute"])
        for call in writes:
            self.assertIn("--latest=false", call)
            self.assertEqual(call[call.index("--target") + 1], "a" * 40)
            self.assertEqual([Path(arg).name for arg in call[3:6]], ["Packages", "Release", "InRelease"])

    def test_update_uploads_inrelease_after_the_files_it_lists(self):
        for ubuntu in UbuntuRelease:
            self.add_apt_release(ubuntu, PREVIOUS)
        result = self.publish()
        self.assertEqual(result.returncode, 0, result.stderr)
        uploads = [(call[2], Path(call[3]).name) for call in self.writes()]
        self.assertEqual(uploads, [(f"apt-{ubuntu.codename}", name) for ubuntu in UbuntuRelease
                                   for name in ("Packages", "Release", "InRelease")])

    def test_unsafe_publishes_stop_before_any_github_write(self):
        cases = {
            "draft": (lambda: self.version_release.update(draft=True), "published"),
            "prerelease release": (lambda: self.version_release.update(prerelease=True), "published"),
            "digest mismatch": (lambda: self.version_release["assets"][0].update(digest="sha256:" + "0" * 64),
                                "digest"),
            "downgrade": (lambda: self.add_apt_release(UbuntuRelease.RESOLUTE, "0.9.0"), "downgrade"),
        }
        original = json.loads(json.dumps(self.state))
        for name, (mutate, reason) in cases.items():
            with self.subTest(name):
                self.state = json.loads(json.dumps(original))
                self.version_release = self.state["releases"][0]
                self.log.unlink(missing_ok=True)
                shutil.rmtree(self.directory / "work", ignore_errors=True)
                mutate()
                result = self.publish()
                self.assertEqual(result.returncode, 1, result.stdout)
                self.assertIn(reason, result.stderr)
                self.assertEqual(self.writes(), [])

    def test_prerelease_tag_is_not_published(self):
        result = self.publish("v0.9.0-rc.1")
        self.assertEqual(result.returncode, 1)
        self.assertIn("Prerelease", result.stderr)
        self.assertEqual(self.writes(), [])


if __name__ == "__main__":
    unittest.main()
