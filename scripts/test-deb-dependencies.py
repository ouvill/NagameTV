#!/usr/bin/env python3
"""Exercise bundled ELF dependency resolution with real dpkg tools; no hardware."""

import importlib.util
from pathlib import Path
import subprocess
import tempfile
import unittest

from package_metadata import UbuntuRelease


SPEC = importlib.util.spec_from_file_location("package_deb", Path(__file__).with_name("package-deb.py"))
PACKAGER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PACKAGER)


class BundledDependenciesTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.work = Path(temporary.name)
        self.stage = self.work / "package"
        (self.stage / "DEBIAN").mkdir(parents=True)
        self.lib = self.stage / "opt/nagametv/usr/lib"
        self.lib.mkdir(parents=True)
        self.binary = self.stage / "opt/nagametv/usr/bin/nagametv"
        self.binary.parent.mkdir()

    def compile(self, source, output, *options):
        subprocess.run(["cc", "-x", "c", "-", "-o", str(output), *options],
                       input=source, text=True, check=True, capture_output=True)

    def build_bundle(self):
        self.compile("int stable(void) { return 0; }", self.lib / "libstable.so.1",
                     "-shared", "-fPIC", "-Wl,-soname,libstable.so.1")
        self.compile("int private(void) { return 0; }", self.lib / "libprivate-16.1.so",
                     "-shared", "-fPIC", "-Wl,-soname,libprivate-16.1.so")
        # A plugin SONAME carries no ABI version, but its dependencies still count.
        self.compile('#include <stdio.h>\nvoid plugin(void) { puts("plugin"); }',
                     self.lib / "libgstexample.so", "-shared", "-fPIC", "-Wl,-soname,libgstexample.so")
        self.compile('#include <stdio.h>\nint stable(void); int private(void);\n'
                     'int main(void) { puts("fixture"); return stable() + private(); }',
                     self.binary, f"-L{self.lib}", "-l:libstable.so.1", "-l:libprivate-16.1.so",
                     "-Wl,-rpath,$ORIGIN/../lib")

    def test_bundled_soname_forms_keep_only_system_dependencies(self):
        self.build_bundle()
        dependencies = PACKAGER.shared_dependencies(self.stage, UbuntuRelease.NOBLE)
        self.assertIn("libc6", dependencies)
        self.assertNotIn("nagametv", dependencies)
        self.assertNotIn("libprivate", dependencies)

    def test_missing_library_is_not_silently_ignored(self):
        self.build_bundle()
        (self.lib / "libstable.so.1").unlink()
        with self.assertRaises(subprocess.CalledProcessError):
            PACKAGER.shared_dependencies(self.stage, UbuntuRelease.NOBLE)


if __name__ == "__main__":
    unittest.main()
