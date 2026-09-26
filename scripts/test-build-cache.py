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

ROOT = Path(__file__).resolve().parent.parent


@unittest.skipUnless(shutil.which("ccache"), "optional ccache is not installed")
class BuildCacheTests(unittest.TestCase):
    def environment(self, **overrides):
        with tempfile.TemporaryDirectory() as directory:
            env = {key: value for key, value in os.environ.items()
                   if key not in ("CC", "CXX", "HOST_CC", "HOST_CXX", "CCACHE_DIR")
                   and not key.startswith("NAGAMETV_BUILD_")}
            env.update(NAGAMETV_BUILD_LOCK_DIR=directory, **overrides)
            output = subprocess.check_output(
                ["bash", str(ROOT / "scripts/with-build-lock.sh"), sys.executable,
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


if __name__ == "__main__":
    unittest.main()
