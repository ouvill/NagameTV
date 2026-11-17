#!/usr/bin/env python3
"""Exercise bundled ELF dependency resolution with real dpkg tools; no hardware."""

from contextlib import redirect_stdout
import hashlib
import io
from pathlib import Path
import subprocess
import struct
import sys
import tempfile
import unittest
from unittest import mock

from scripts.packaging.metadata import Architecture, UbuntuRelease


from scripts.packaging import package_deb as PACKAGER


ELF_IDENT_BYTES = 16
ELF_TYPE_EXECUTABLE = 2
ELF_VERSION_CURRENT = 1
ELF_CLASS_64 = 2
ELF_DATA_LITTLE_ENDIAN = 1
ELF_MACHINE_X86_64 = 62
ELF_MACHINE_AARCH64 = 183


def elf_header(architecture: Architecture, elf_class=ELF_CLASS_64, byte_order=ELF_DATA_LITTLE_ENDIAN):
    machine = {Architecture.X86_64: ELF_MACHINE_X86_64, Architecture.AARCH64: ELF_MACHINE_AARCH64}[architecture]
    identity = b"\x7fELF" + bytes([elf_class, byte_order, ELF_VERSION_CURRENT])
    return struct.pack("<16sHH", identity.ljust(ELF_IDENT_BYTES, b"\0"), ELF_TYPE_EXECUTABLE, machine)


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
        dependencies = PACKAGER.shared_dependencies(self.stage, UbuntuRelease.NOBLE, Architecture.native())
        self.assertIn("libc6", dependencies)
        self.assertNotIn("nagametv", dependencies)
        self.assertNotIn("libprivate", dependencies)

    def test_missing_library_is_not_silently_ignored(self):
        self.build_bundle()
        (self.lib / "libstable.so.1").unlink()
        with self.assertRaises(subprocess.CalledProcessError):
            PACKAGER.shared_dependencies(self.stage, UbuntuRelease.NOBLE, Architecture.native())

    def test_package_cannot_mix_arm_and_x86_elf_files(self):
        for architecture in Architecture:
            with self.subTest(architecture=architecture):
                self.binary.write_bytes(elf_header(architecture))
                self.assertEqual(PACKAGER.elf_files(self.stage, architecture), [self.binary])
                other = next(arch for arch in Architecture if arch != architecture)
                with self.assertRaisesRegex(ValueError, "ELF architecture"):
                    PACKAGER.elf_files(self.stage, other)

    def test_wrong_elf_class_endianness_and_truncated_header_are_rejected(self):
        elf_class_32 = 1
        big_endian = 2
        for invalid in (elf_header(Architecture.AARCH64)[:-1],
                        elf_header(Architecture.AARCH64, elf_class=elf_class_32),
                        elf_header(Architecture.AARCH64, byte_order=big_endian)):
            with self.subTest(header=invalid):
                self.binary.write_bytes(invalid)
                with self.assertRaisesRegex(ValueError, "ELF architecture"):
                    PACKAGER.elf_files(self.stage, Architecture.AARCH64)

    def test_arm_package_control_filename_and_checksum(self):
        stage = self.work / "arm-package"
        binary = stage / "usr/bin/nagametv"
        binary.parent.mkdir(parents=True)
        binary.write_bytes(elf_header(Architecture.AARCH64))
        output = self.work / "output"
        args = ["package_deb", "26.04", str(stage), str(output), "--architecture", "aarch64"]
        # Dependency resolution is covered with real ELF files above; this fixture
        # checks dpkg's packaging metadata without executing a foreign binary.
        with mock.patch.object(sys, "argv", args), \
                mock.patch.object(PACKAGER, "shared_dependencies", return_value="libc6") as dependencies, \
                redirect_stdout(io.StringIO()):
            PACKAGER.main()
        dependencies.assert_called_once_with(stage, UbuntuRelease.RESOLUTE, Architecture.AARCH64)
        package, = output.glob("*.deb")
        self.assertTrue(package.name.endswith("-1ubuntu26.04_arm64.deb"))
        architecture = subprocess.check_output(["dpkg-deb", "--field", package, "Architecture"], text=True)
        self.assertEqual(architecture.strip(), "arm64")
        checksum = hashlib.sha256(package.read_bytes()).hexdigest()
        self.assertEqual(package.with_suffix(".deb.sha256").read_text(), f"{checksum}  {package.name}\n")


if __name__ == "__main__":
    unittest.main()
