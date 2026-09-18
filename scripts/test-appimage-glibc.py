#!/usr/bin/env python3
"""Hardware-free regression checks using real ELF versioned dependencies."""
from pathlib import Path
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parent.parent
BASELINE = "2.39"
NEWER_GLIBC = "2.43"


class AppImageGlibc(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.work = Path(self.temporary.name)
        self.app_dir = self.work / "AppDir"
        self.app_dir.mkdir()

    def compile(self, *arguments):
        subprocess.run(["cc", "-shared", "-fPIC", "-nostdlib", *map(str, arguments)],
                       cwd=self.work, check=True, capture_output=True)

    def dependency(self, required):
        # Provider stays outside AppDir. Only the plugin's version needs are
        # scanned, just as the real bundle relies on the system glibc.
        (self.work / "provider.c").write_text("int baseline_symbol(void) { return 0; }\n")
        (self.work / "versions.map").write_text(f"GLIBC_{required} {{ global: baseline_symbol; }};\n")
        self.compile("provider.c", "-Wl,--version-script=versions.map",
                     "-Wl,-soname,libbaseline.so", "-o", "libbaseline.so")
        (self.work / "plugin.c").write_text(
            "extern int baseline_symbol(void); int plugin(void) { return baseline_symbol(); }\n")
        plugin = self.app_dir / "usr/lib/plugins/plugin.so"
        plugin.parent.mkdir(parents=True)
        self.compile("plugin.c", "-L.", "-lbaseline", "-o", plugin)

    def check_bundle(self):
        return subprocess.run(
            ["python3", str(ROOT / "scripts/check-appimage-glibc.py"),
             str(self.app_dir), "--max-glibc", BASELINE],
            text=True, capture_output=True,
        )

    def test_accepts_baseline_and_ignores_non_elf_resources(self):
        self.dependency(BASELINE)
        (self.app_dir / "resource.qml").write_text(f"// GLIBC_{NEWER_GLIBC}\n")
        result = self.check_bundle()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn(f"highest glibc requirement: {BASELINE}", result.stdout)

    def test_rejects_newer_requirement_in_nested_plugin(self):
        self.dependency(NEWER_GLIBC)
        result = self.check_bundle()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn(f"usr/lib/plugins/plugin.so: requires GLIBC_{NEWER_GLIBC}", result.stderr)

    def test_definitions_are_not_requirements(self):
        self.dependency(NEWER_GLIBC)
        (self.app_dir / "usr/lib/plugins/plugin.so").unlink()
        (self.work / "libbaseline.so").rename(self.app_dir / "provider.so")
        result = self.check_bundle()
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_rejects_private_requirement(self):
        self.dependency("PRIVATE")
        result = self.check_bundle()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("unsupported requirement GLIBC_PRIVATE", result.stderr)

    def test_rejects_invalid_elf(self):
        (self.app_dir / "broken.so").write_bytes(b"\x7fELFtruncated")
        self.assertNotEqual(self.check_bundle().returncode, 0)

    def test_rejects_empty_bundle(self):
        result = self.check_bundle()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("No ELF files", result.stderr)


if __name__ == "__main__":
    unittest.main()
