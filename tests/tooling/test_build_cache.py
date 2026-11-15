#!/usr/bin/env python3
"""Check compiler selection and Cargo cache reuse without Qt or hardware."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

from scripts.build.support import ensure_lock, lock_fds

ROOT = Path(__file__).resolve().parents[2]
from scripts.build import fresh_local_crates as fresh


@unittest.skipUnless(shutil.which("ccache"), "optional ccache is not installed")
class BuildCacheTests(unittest.TestCase):
    def environment(self, **overrides):
        with tempfile.TemporaryDirectory() as directory:
            env = {key: value for key, value in os.environ.items()
                   if key not in ("CC", "CXX", "HOST_CC", "HOST_CXX", "CCACHE_DIR")
                   and not key.startswith("NAGAMETV_BUILD_")}
            env.update(NAGAMETV_BUILD_LOCK_DIR=directory, **overrides)
            output = subprocess.check_output(
                ["bash", str(ROOT / "scripts/build/with-build-lock.sh"), sys.executable,
                 "-c", "import json, os; print(json.dumps(dict(os.environ)))"],
                cwd=ROOT, env=env, text=True)
            return json.loads(output)

    def test_native_defaults_and_shared_cache(self):
        env = self.environment()
        self.assertEqual(env["HOST_CC"], "ccache cc")
        self.assertEqual(env["HOST_CXX"], "ccache c++")
        self.assertEqual(env["CCACHE_DIR"], str(ROOT / "build/ccache"))
        # Generic CC/CXX would also override cc-rs's cross compiler selection.
        self.assertNotIn("CC", env)
        self.assertNotIn("CXX", env)

    def test_explicit_compilers_and_cache_directory(self):
        env = self.environment(CC="custom-cc", CXX="custom-cxx",
                               CCACHE_DIR="/tmp/custom-cache")
        self.assertEqual(env["CC"], "custom-cc")
        self.assertEqual(env["CXX"], "custom-cxx")
        self.assertNotIn("HOST_CC", env)
        self.assertNotIn("HOST_CXX", env)
        self.assertEqual(env["CCACHE_DIR"], "/tmp/custom-cache")

    def test_host_and_target_overrides(self):
        env = self.environment(HOST_CC="host-cc", HOST_CXX="host-cxx",
                               TARGET_CXX="cross-cxx", CXXFLAGS="-DUSER_FLAG=1")
        self.assertEqual(env["HOST_CC"], "host-cc")
        self.assertEqual(env["HOST_CXX"], "host-cxx")
        self.assertEqual(env["TARGET_CXX"], "cross-cxx")
        self.assertEqual(env["CXXFLAGS"], "-DUSER_FLAG=1")


class RestoredCargoCacheTests(unittest.TestCase):
    def environment(self, root):
        env = {key: value for key, value in os.environ.items()
               if not key.startswith("CARGO_") or key in ("CARGO_HOME", "CARGO_BUILD_JOBS")}
        return dict(env, CARGO_TARGET_DIR=str(root / "target"), CARGO_NET_OFFLINE="true")

    def build(self, manifest, env, profile):
        output = subprocess.check_output(
            ["cargo", "build", "--manifest-path", str(manifest), "--profile", profile,
             "--message-format=json-render-diagnostics"],
            cwd=manifest.parent, env=env, text=True, pass_fds=lock_fds())
        artifacts = [item for line in output.splitlines()
                     if (item := json.loads(line)).get("reason") == "compiler-artifact"]
        # Cargo's intermediate directory layout is not a stable interface.
        dependencies = {Path(filename): Path(filename).stat().st_mtime_ns
                        for item in artifacts if item["target"]["name"] == "jobserver"
                        for filename in item["filenames"] if filename.endswith(".rlib")}
        self.assertEqual(len(dependencies), 1, "expected one compiled registry dependency")
        executables = [item["executable"] for item in artifacts if item.get("executable")]
        return dependencies, executables

    def test_release_dependencies_are_reused_across_standalone_roots(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            env = self.environment(root)
            artifacts = None
            for name in ("application", "standalone"):
                if name == "standalone":
                    # Match the application's existing debug=1 artifacts, not
                    # merely two roots overridden by the same new setting.
                    (root / ".cargo").mkdir()
                    shutil.copyfile(ROOT / ".cargo/config.toml", root / ".cargo/config.toml")
                crate = root / name
                crate.mkdir()
                manifest = crate / "Cargo.toml"
                manifest.write_text(
                    f'[package]\nname="{name}"\nversion="0.0.0"\nedition="2024"\n'
                    '[lib]\npath="lib.rs"\n[dependencies]\njobserver="=0.1.35"\n'
                    + ('[profile.release]\ndebug=1\n' if name == "application" else ''))
                (crate / "lib.rs").write_text("pub fn client() -> jobserver::Client { jobserver::Client::new(1).unwrap() }\n")
                current, _executables = self.build(manifest, env, "release")
                if artifacts is not None:
                    self.assertEqual(current, artifacts, "profile mismatch rebuilt the shared dependency")
                artifacts = current

    def test_changed_path_dependency_with_old_mtime_and_registry_reuse(self):
        for profile in ("dev", "release"):
            with self.subTest(profile=profile), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                manifest = root / "Cargo.toml"
                manifest.write_text(
                    '[package]\nname="cache-fixture"\nversion="0.0.0"\nedition="2024"\n'
                    '[[bin]]\nname="cache-fixture"\npath="main.rs"\n'
                    '[dependencies]\nlocal-value={path="local"}\njobserver="=0.1.35"\n')
                (root / "main.rs").write_text(
                    'fn main(){ let _client = jobserver::Client::new(1).unwrap(); '
                    'println!("{}", local_value::value()); }\n')
                (root / "local").mkdir()
                (root / "local/Cargo.toml").write_text(
                    '[package]\nname="local-value"\nversion="0.0.0"\nedition="2024"\n'
                    '[lib]\npath="lib.rs"\n')
                source = root / "local/lib.rs"
                source.write_text("pub fn value() -> u8 { 1 }\n")
                original = source.stat()
                env = self.environment(root)
                dependencies, executables = self.build(manifest, env, profile)
                self.assertEqual(len(executables), 1)
                self.assertEqual(subprocess.check_output(executables, text=True).strip(), "1")
                source.write_text("pub fn value() -> u8 { 2 }\n")
                # Simulate a checkout older than the restored build artifacts.
                os.utime(source, ns=(original.st_atime_ns, original.st_mtime_ns))
                fresh.clean_local_packages(manifest, env)
                self.assertEqual({path: path.stat().st_mtime_ns for path in dependencies}, dependencies)
                rebuilt, executables = self.build(manifest, env, profile)
                self.assertEqual(rebuilt, dependencies)
                self.assertEqual(len(executables), 1)
                self.assertEqual(subprocess.check_output(executables, text=True).strip(), "2")


if __name__ == "__main__":
    ensure_lock()
    unittest.main()
