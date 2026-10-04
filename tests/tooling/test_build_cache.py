#!/usr/bin/env python3
"""Check compiler selection without requiring Qt or hardware."""
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
    def test_release_dependencies_are_reused_across_standalone_roots(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            env = dict(os.environ, CARGO_TARGET_DIR=str(root / "target"), CARGO_NET_OFFLINE="true")
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
                subprocess.run(["cargo", "build", "--manifest-path", str(manifest), "--release"],
                               cwd=root, env=env, check=True, pass_fds=lock_fds())
                current = {path.name: path.stat().st_mtime_ns
                           for path in (root / "target/release/deps").glob("libjobserver-*.rlib")}
                self.assertEqual(len(current), 1, "profile mismatch rebuilt the shared dependency")
                if artifacts is not None:
                    self.assertEqual(current, artifacts)
                artifacts = current

    def test_changed_path_dependency_with_old_mtime_and_registry_reuse(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            manifest = root / "Cargo.toml"
            manifest.write_text(
                '[package]\nname="cache-fixture"\nversion="0.0.0"\nedition="2024"\n'
                '[[bin]]\nname="cache-fixture"\npath="main.rs"\n'
                '[dependencies]\nlocal-value={path="local"}\njobserver="=0.1.35"\n')
            (root / "main.rs").write_text('fn main(){ println!("{}", local_value::value()); }\n')
            (root / "local").mkdir()
            (root / "local/Cargo.toml").write_text(
                '[package]\nname="local-value"\nversion="0.0.0"\nedition="2024"\n'
                '[lib]\npath="lib.rs"\n')
            source = root / "local/lib.rs"
            source.write_text("pub fn value() -> u8 { 1 }\n")
            original = source.stat()
            env = dict(os.environ, CARGO_TARGET_DIR=str(root / "target"), CARGO_NET_OFFLINE="true")
            command = ["cargo", "run", "--quiet", "--manifest-path", str(manifest)]
            def run():
                return subprocess.check_output(command, env=env, text=True, pass_fds=lock_fds()).strip()
            self.assertEqual(run(), "1")
            dependencies = list((root / "target/debug/deps").glob("libjobserver-*.rlib"))
            self.assertTrue(dependencies)
            modified = {path: path.stat().st_mtime_ns for path in dependencies}
            source.write_text("pub fn value() -> u8 { 2 }\n")
            # Simulate a checkout older than the restored build artifacts.
            os.utime(source, ns=(original.st_atime_ns, original.st_mtime_ns))
            fresh.clean_local_packages(manifest, env)
            self.assertEqual({path: path.stat().st_mtime_ns for path in dependencies}, modified)
            self.assertEqual(run(), "2")


if __name__ == "__main__":
    ensure_lock()
    unittest.main()
