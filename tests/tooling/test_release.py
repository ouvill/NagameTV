#!/usr/bin/env python3
"""Hardware-free release checks; GitHub writes are captured by a local fixture."""

import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

from scripts.packaging.metadata import (UbuntuRelease, appimage_filename, deb_filename, debian_version,
                                        release_assets)


ROOT = Path(__file__).resolve().parents[2]
VERSION = "0.1.0"
SUBMODULE_COMMIT = "1" * 40


class ReleaseTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.project = Path(temporary.name)
        for directory in ("scripts/packaging", "rust", "packaging/linux", "packaging/flatpak", "assets", "bin"):
            (self.project / directory).mkdir(parents=True)
        for script in ("check_release_metadata.py", "create-release.sh", "metadata.py"):
            shutil.copyfile(ROOT / "scripts/packaging" / script, self.project / "scripts/packaging" / script)
        subprocess.run(["git", "init", "--quiet", str(self.project)], check=True)
        sources = []
        for name in ("libaribcaption", "tsreadex"):
            path = f"third_party/{name}"
            subprocess.run(["git", "update-index", "--add", "--cacheinfo",
                            f"160000,{SUBMODULE_COMMIT},{path}"], cwd=self.project, check=True)
            sources.append({"type": "git", "dest": path, "commit": SUBMODULE_COMMIT})
        self.manifest = self.project / "packaging/flatpak/io.github.ouvill.nagametv.json"
        self.manifest.write_text(json.dumps({"modules": [{"sources": sources}]}))
        self.set_version(VERSION)
        self.log = self.project / "gh.jsonl"
        gh = self.project / "bin/gh"
        gh.write_text("""#!/usr/bin/env python3
import json, os, sys
from pathlib import Path
with Path(os.environ['FAKE_GH_LOG']).open('a') as log:
    log.write(json.dumps(sys.argv[1:]) + '\\n')
if sys.argv[1] == 'api':
    if os.environ.get('FAKE_API_ERROR'):
        sys.exit(1)
    print(os.environ.get('FAKE_RELEASE_STATE', ''))
""")
        gh.chmod(0o755)
        self.env = dict(os.environ, PATH=f"{self.project}/bin:{os.environ['PATH']}",
                        GH_REPO="example/nagametv", FAKE_GH_LOG=str(self.log))

    def set_version(self, version):
        for filename, prefix in (("Cargo.toml", "[package]"), ("Cargo.lock", "[[package]]")):
            (self.project / "rust" / filename).write_text(
                f'{prefix}\nname = "nagametv"\nversion = "{version}"\n')
        (self.project / "packaging/linux/io.github.ouvill.nagametv.metainfo.xml").write_text(
            f'<component><releases><release version="{version}"/></releases></component>')

    def metadata(self, tag):
        return subprocess.run(["python3", "-m", "scripts.packaging.check_release_metadata", "--tag", tag,
                               "--github-output", "outputs"], cwd=self.project,
                              text=True, capture_output=True)

    def bundles(self, version=VERSION):
        for name in release_assets(version):
            data = f"test bundle: {name}".encode()
            (self.project / "assets" / name).write_bytes(data)
            (self.project / "assets" / f"{name}.sha256").write_text(
                f"{hashlib.sha256(data).hexdigest()}  {name}\n")
        appimage = appimage_filename(version)
        self.zsync(appimage, (self.project / "assets" / appimage).read_bytes())

    def zsync(self, appimage, described: bytes):
        # Matches the text header that zsyncmake writes before its binary checksums.
        (self.project / "assets" / f"{appimage}.zsync").write_bytes(
            f"zsync: 0.6.2\nFilename: {appimage}\nURL: {appimage}\n"
            f"SHA-1: {hashlib.sha1(described).hexdigest()}\n\n".encode() + b"\0\x01binary")

    def release(self, version=VERSION):
        args = ["bash", "scripts/packaging/create-release.sh", f"v{version}", "assets"]
        return subprocess.run(args, cwd=self.project, env=self.env, text=True, capture_output=True)

    def calls(self):
        return [json.loads(line) for line in self.log.read_text().splitlines()] if self.log.exists() else []

    def writes(self):
        return [call for call in self.calls()
                if (call[0] == "api" and "--method" in call)
                or (call[0] == "release" and call[1] != "view")]

    def test_valid_versions(self):
        for version in (VERSION, "1.2.3-rc.1", "1.2.3+build.5", "1.2.3-beta.2+build.5"):
            with self.subTest(version=version):
                self.set_version(version)
                result = self.metadata(f"v{version}")
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertIn(f"version={version}\n", (self.project / "outputs").read_text())

    def test_invalid_tags_do_not_produce_outputs(self):
        for tag in ("0.1.0", "v0.2.0", "v0.1.0\nversion=other", "v0.1.0/../../file"):
            with self.subTest(tag=tag):
                self.assertNotEqual(self.metadata(tag).returncode, 0)
                self.assertFalse((self.project / "outputs").exists())

    def test_invalid_semver_is_rejected(self):
        for version in ("01.2.3", "1.2.3-01", "../1.2.3"):
            with self.subTest(version=version):
                self.set_version(version)
                self.assertNotEqual(self.metadata(f"v{version}").returncode, 0)

    def test_stale_version_metadata(self):
        for path in ("rust/Cargo.lock", "packaging/linux/io.github.ouvill.nagametv.metainfo.xml"):
            with self.subTest(path=path):
                self.set_version(VERSION)
                target = self.project / path
                target.write_text(target.read_text().replace(VERSION, "0.0.9"))
                self.assertNotEqual(self.metadata(f"v{VERSION}").returncode, 0)

    def test_submodule_mismatch(self):
        self.manifest.write_text(self.manifest.read_text().replace(SUBMODULE_COMMIT, "2" * 40))
        result = self.metadata(f"v{VERSION}")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("differs from submodule", result.stderr)

    def test_create_draft_with_all_bundles(self):
        self.bundles()
        result = self.release()
        self.assertEqual(result.returncode, 0, result.stderr)
        create = self.calls()[-1]
        self.assertEqual(create[:3], ["release", "create", f"v{VERSION}"])
        for argument in ("--draft", "--verify-tag", "--generate-notes"):
            self.assertIn(argument, create)
        self.assertNotIn("--prerelease", create)
        for name in release_assets(VERSION):
            self.assertIn(name, create)
            self.assertIn(f"{name}.sha256", create)
        self.assertIn(f"{appimage_filename(VERSION)}.zsync", create)

    def test_prerelease_draft(self):
        version = "1.0.0-rc.1"
        self.set_version(version)
        self.bundles(version)
        result = self.release(version)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("--prerelease", self.calls()[-1])

    def test_retry_updates_existing_draft(self):
        self.bundles()
        self.env["FAKE_RELEASE_STATE"] = "true"
        result = self.release()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.calls()[-1][:3], ["release", "upload", f"v{VERSION}"])
        self.assertIn("--clobber", self.calls()[-1])

    def test_published_release_is_not_modified(self):
        self.bundles()
        self.env["FAKE_RELEASE_STATE"] = "false"
        result = self.release()
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(len(self.calls()), 1)
        self.assertEqual(self.writes(), [])
        self.assertIn("already published", result.stderr)

    def test_api_failure_is_not_treated_as_missing_release(self):
        self.bundles()
        self.env["FAKE_API_ERROR"] = "1"
        self.assertNotEqual(self.release().returncode, 0)
        self.assertEqual(self.writes(), [])

    def test_missing_bundle_prevents_api_calls(self):
        for name in release_assets(VERSION):
            for asset in (name, f"{name}.sha256"):
                with self.subTest(asset=asset):
                    self.bundles()
                    (self.project / "assets" / asset).unlink()
                    self.assertNotEqual(self.release().returncode, 0)
                    self.assertEqual(self.calls(), [])

    def test_corrupt_bundle_prevents_api_calls(self):
        for name in release_assets(VERSION):
            with self.subTest(asset=name):
                self.bundles()
                (self.project / "assets" / name).write_bytes(b"corrupt")
                self.assertNotEqual(self.release().returncode, 0)
                self.assertEqual(self.calls(), [])

    def test_missing_appimage_zsync_prevents_api_calls(self):
        self.bundles()
        (self.project / "assets" / f"{appimage_filename(VERSION)}.zsync").unlink()
        result = self.release()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Missing release asset", result.stderr)
        self.assertEqual(self.calls(), [])

    def test_zsync_for_another_appimage_prevents_api_calls(self):
        for described in (b"previous AppImage", None):
            with self.subTest(described=described):
                self.bundles()
                appimage = appimage_filename(VERSION)
                if described is None:
                    (self.project / "assets" / f"{appimage}.zsync").write_text("not a zsync file\n")
                else:
                    self.zsync(appimage, described)
                result = self.release()
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("zsync does not match", result.stderr)
                self.assertEqual(self.calls(), [])

    def test_debian_versions_and_upgrade_order(self):
        for ubuntu in UbuntuRelease:
            final = debian_version("1.2.3", ubuntu)
            prerelease = debian_version("1.2.3-rc.1", ubuntu)
            self.assertEqual(final, f"1.2.3-1ubuntu{ubuntu.value}")
            self.assertEqual(prerelease, f"1.2.3~rc.1-1ubuntu{ubuntu.value}")
            self.assertEqual(deb_filename("1.2.3", ubuntu), f"nagametv_{final}_amd64.deb")
            subprocess.run(["dpkg", "--compare-versions", prerelease, "lt", final], check=True)
            subprocess.run(["dpkg", "--validate-version",
                            debian_version("1.2.3-rc.1+build.5", ubuntu)], check=True)
        subprocess.run(["dpkg", "--compare-versions",
                        debian_version(VERSION, UbuntuRelease.NOBLE), "lt",
                        debian_version(VERSION, UbuntuRelease.RESOLUTE)], check=True)


if __name__ == "__main__":
    unittest.main()
