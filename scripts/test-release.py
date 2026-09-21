#!/usr/bin/env python3
"""Hardware-free release checks; GitHub writes are captured by a local fixture."""

import hashlib
from enum import Enum
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

from package_metadata import UbuntuRelease, deb_filename, debian_version, release_assets


ROOT = Path(__file__).resolve().parent.parent
VERSION = "0.1.0"
SUBMODULE_COMMIT = "1" * 40


class ReleaseMode(Enum):
    TAG = "tag"
    LATEST_BUILD = "latest-build"


class ReleaseTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.project = Path(temporary.name)
        for directory in ("scripts", "rust", "packaging/linux", "packaging/flatpak", "assets", "bin"):
            (self.project / directory).mkdir(parents=True)
        for script in ("check-release-metadata.py", "create-release.sh", "package_metadata.py"):
            shutil.copyfile(ROOT / "scripts" / script, self.project / "scripts" / script)
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
        subprocess.run(["git", "-c", "user.name=Release Test", "-c", "user.email=test@example.com",
                        "-c", "commit.gpgsign=false", "commit", "--quiet", "-m", "Test checkout"],
                       cwd=self.project, check=True)
        self.head = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=self.project,
                                            text=True).strip()
        self.log = self.project / "gh.jsonl"
        gh = self.project / "bin/gh"
        gh.write_text("""#!/usr/bin/env python3
import json, os, sys
from pathlib import Path
with Path(os.environ['FAKE_GH_LOG']).open('a') as log:
    log.write(json.dumps(sys.argv[1:]) + '\\n')
failure = os.environ.get('FAKE_FAIL_COMMAND')
if failure and sys.argv[1:1 + len(json.loads(failure))] == json.loads(failure):
    sys.exit(1)
if sys.argv[1] == 'api':
    if os.environ.get('FAKE_API_ERROR'):
        sys.exit(1)
    if any(arg.endswith('/git/ref/heads/main') for arg in sys.argv):
        print(os.environ['FAKE_MAIN_COMMIT'])
    elif any('/git/matching-refs/' in arg for arg in sys.argv):
        print(os.environ.get('FAKE_TAG_COMMIT', ''))
    elif '--method' not in sys.argv:
        print(os.environ.get('FAKE_RELEASE_STATE', ''))
elif sys.argv[1:3] == ['release', 'view']:
    print(os.environ.get('FAKE_ASSETS', ''))
""")
        gh.chmod(0o755)
        self.env = dict(os.environ, PATH=f"{self.project}/bin:{os.environ['PATH']}",
                        GH_REPO="example/nagametv", FAKE_GH_LOG=str(self.log),
                        FAKE_MAIN_COMMIT=self.head)

    def set_version(self, version):
        for filename, prefix in (("Cargo.toml", "[package]"), ("Cargo.lock", "[[package]]")):
            (self.project / "rust" / filename).write_text(
                f'{prefix}\nname = "nagametv"\nversion = "{version}"\n')
        (self.project / "packaging/linux/io.github.ouvill.nagametv.metainfo.xml").write_text(
            f'<component><releases><release version="{version}"/></releases></component>')

    def metadata(self, tag):
        return subprocess.run(["python3", "scripts/check-release-metadata.py", "--tag", tag,
                               "--github-output", "outputs"], cwd=self.project,
                              text=True, capture_output=True)

    def bundles(self, version=VERSION):
        for name in release_assets(version):
            data = f"test bundle: {name}".encode()
            (self.project / "assets" / name).write_bytes(data)
            (self.project / "assets" / f"{name}.sha256").write_text(
                f"{hashlib.sha256(data).hexdigest()}  {name}\n")

    def release(self, version=VERSION, mode=ReleaseMode.TAG):
        args = ["bash", "scripts/create-release.sh", f"v{version}", "assets"]
        match mode:
            case ReleaseMode.TAG:
                pass
            case ReleaseMode.LATEST_BUILD:
                args.append("--latest-build")
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

    def test_prerelease_draft(self):
        version = "1.0.0-rc.1"
        self.set_version(version)
        self.bundles(version)
        result = self.release(version)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("--prerelease", self.calls()[-1])

    def test_latest_build_creates_and_publishes_all_bundles(self):
        self.bundles()
        result = self.release(mode=ReleaseMode.LATEST_BUILD)
        self.assertEqual(result.returncode, 0, result.stderr)
        tag, create, publish = self.writes()
        self.assertEqual(tag[:4], ["api", "--method", "POST", "repos/example/nagametv/git/refs"])
        self.assertIn("ref=refs/tags/latest-build", tag)
        self.assertIn(f"sha={self.head}", tag)
        self.assertEqual(create[:3], ["release", "create", "latest-build"])
        for argument in ("--draft", "--prerelease", "--latest=false", "--verify-tag"):
            self.assertIn(argument, create)
        self.assertIn(self.head[:12], create[create.index("--title") + 1])
        for name in release_assets(VERSION):
            self.assertIn(name, create)
            self.assertIn(f"{name}.sha256", create)
        self.assertEqual(publish[:3], ["release", "edit", "latest-build"])
        for argument in ("--draft=false", "--prerelease", "--latest=false"):
            self.assertIn(argument, publish)
        self.assertEqual(publish[publish.index("--target") + 1], self.head)
        self.assertIn(self.head, publish[publish.index("--notes") + 1])

    def test_latest_build_overwrites_and_removes_old_version_assets(self):
        self.bundles()
        stale_assets = [asset for name in release_assets("0.0.9") for asset in (name, f"{name}.sha256")]
        current_assets = [asset for name in release_assets(VERSION) for asset in (name, f"{name}.sha256")]
        self.env.update(FAKE_RELEASE_STATE="prerelease", FAKE_TAG_COMMIT=SUBMODULE_COMMIT,
                        FAKE_ASSETS="\n".join(stale_assets + current_assets))
        result = self.release(mode=ReleaseMode.LATEST_BUILD)
        self.assertEqual(result.returncode, 0, result.stderr)
        hide, tag, upload, *deletions, publish = self.writes()
        self.assertEqual(hide, ["release", "edit", "latest-build", "--draft=true"])
        self.assertEqual(tag[:4], ["api", "--method", "PATCH",
                                   "repos/example/nagametv/git/refs/tags/latest-build"])
        self.assertIn(f"sha={self.head}", tag)
        self.assertIn("force=true", tag)
        self.assertEqual(upload[:3], ["release", "upload", "latest-build"])
        self.assertIn("--clobber", upload)
        for asset in current_assets:
            self.assertIn(asset, upload)
        self.assertEqual(deletions, [["release", "delete-asset", "--yes", "--", "latest-build", name]
                                     for name in stale_assets])
        self.assertIn("--draft=false", publish)

    def test_latest_build_rerun_repairs_partial_release(self):
        self.bundles()
        self.env.update(FAKE_RELEASE_STATE="prerelease", FAKE_TAG_COMMIT=self.head,
                        FAKE_ASSETS=release_assets(VERSION)[0])
        result = self.release(mode=ReleaseMode.LATEST_BUILD)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse(any(call[:2] == ["release", "create"] for call in self.calls()))
        upload = next(call for call in self.calls() if call[:2] == ["release", "upload"])
        for name in release_assets(VERSION):
            self.assertIn(name, upload)
            self.assertIn(f"{name}.sha256", upload)
        self.assertIn("--draft=false", self.calls()[-1])

    def test_outdated_build_does_not_write_to_github(self):
        self.bundles()
        self.env["FAKE_MAIN_COMMIT"] = SUBMODULE_COMMIT
        result = self.release(mode=ReleaseMode.LATEST_BUILD)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("Skipping outdated build", result.stdout)
        self.assertEqual(self.writes(), [])

    def test_invalid_main_response_does_not_write_to_github(self):
        self.bundles()
        self.env["FAKE_MAIN_COMMIT"] = ""
        self.assertNotEqual(self.release(mode=ReleaseMode.LATEST_BUILD).returncode, 0)
        self.assertEqual(self.writes(), [])

    def test_latest_build_refuses_stable_or_immutable_release(self):
        self.bundles()
        for state in ("release", "immutable", "unexpected"):
            with self.subTest(state=state):
                self.env["FAKE_RELEASE_STATE"] = state
                self.assertNotEqual(self.release(mode=ReleaseMode.LATEST_BUILD).returncode, 0)
                self.assertEqual(self.writes(), [])

    def test_latest_build_read_failures_prevent_writes(self):
        self.bundles()
        self.env["FAKE_RELEASE_STATE"] = "prerelease"
        for command in (["api", "--paginate"],
                        ["api", "repos/example/nagametv/git/matching-refs/tags/latest-build"],
                        ["release", "view"]):
            with self.subTest(command=command):
                self.env["FAKE_FAIL_COMMAND"] = json.dumps(command)
                self.assertNotEqual(self.release(mode=ReleaseMode.LATEST_BUILD).returncode, 0)
                self.assertEqual(self.writes(), [])

    def test_latest_build_write_failures_prevent_publication(self):
        self.bundles()
        self.env.update(FAKE_RELEASE_STATE="prerelease", FAKE_TAG_COMMIT=SUBMODULE_COMMIT,
                        FAKE_ASSETS="old.AppImage")
        for command in (["release", "edit", "latest-build", "--draft=true"],
                        ["api", "--method", "PATCH"], ["release", "upload"],
                        ["release", "delete-asset"]):
            with self.subTest(command=command):
                self.env["FAKE_FAIL_COMMAND"] = json.dumps(command)
                self.assertNotEqual(self.release(mode=ReleaseMode.LATEST_BUILD).returncode, 0)
                self.assertFalse(any("--draft=false" in call for call in self.writes()))

    def test_latest_build_create_failure_prevents_publication(self):
        self.bundles()
        self.env["FAKE_FAIL_COMMAND"] = json.dumps(["release", "create"])
        self.assertNotEqual(self.release(mode=ReleaseMode.LATEST_BUILD).returncode, 0)
        self.assertFalse(any("--draft=false" in call for call in self.writes()))

    def test_main_candidate_requires_a_commit(self):
        self.bundles()
        subprocess.run(["git", "symbolic-ref", "HEAD", "refs/heads/unborn"],
                       cwd=self.project, check=True)
        self.assertNotEqual(self.release(mode=ReleaseMode.LATEST_BUILD).returncode, 0)
        self.assertEqual(self.calls(), [])

    def test_main_candidate_validates_package_version(self):
        self.bundles()
        result = self.release(version="0.2.0", mode=ReleaseMode.LATEST_BUILD)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("must match Cargo.toml", result.stderr)
        self.assertEqual(self.calls(), [])

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
        for mode in ReleaseMode:
            with self.subTest(mode=mode):
                self.assertNotEqual(self.release(mode=mode).returncode, 0)
                self.assertEqual(self.writes(), [])

    def test_missing_bundle_prevents_api_calls(self):
        for name in release_assets(VERSION):
            for asset in (name, f"{name}.sha256"):
                with self.subTest(asset=asset):
                    self.bundles()
                    (self.project / "assets" / asset).unlink()
                    for mode in ReleaseMode:
                        self.assertNotEqual(self.release(mode=mode).returncode, 0)
                    self.assertEqual(self.calls(), [])

    def test_corrupt_bundle_prevents_api_calls(self):
        for name in release_assets(VERSION):
            with self.subTest(asset=name):
                self.bundles()
                (self.project / "assets" / name).write_bytes(b"corrupt")
                for mode in ReleaseMode:
                    self.assertNotEqual(self.release(mode=mode).returncode, 0)
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
