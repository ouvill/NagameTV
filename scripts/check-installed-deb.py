#!/usr/bin/env python3
"""Check installed deb contents and linkage; never initialize Qt or GStreamer."""

import os
from pathlib import Path
import subprocess
import sys

from package_metadata import UbuntuRelease


def main():
    ubuntu = UbuntuRelease(sys.argv[1])
    environment = dict(os.environ)
    match ubuntu:
        case UbuntuRelease.NOBLE:
            environment["LD_LIBRARY_PATH"] = "/opt/nagametv/usr/lib"
        case UbuntuRelease.RESOLUTE:
            if Path("/opt/nagametv").exists():
                raise RuntimeError("Native package must not contain a bundled runtime")
    paths = subprocess.check_output(["dpkg-query", "-L", "nagametv"], text=True).splitlines()
    count = 0
    for name in paths:
        path = Path(name)
        if not path.is_file() or path.is_symlink():
            continue
        with path.open("rb") as stream:
            if stream.read(4) != b"\x7fELF":
                continue
        result = subprocess.run(["ldd", str(path)], env=environment, text=True, capture_output=True)
        if result.returncode or "not found" in result.stdout + result.stderr:
            raise RuntimeError(f"Unresolved ELF dependencies: {path}\n{result.stdout}{result.stderr}")
        count += 1
    if count == 0:
        raise RuntimeError("Package contains no ELF files")
    # This invalid feature is parsed before QGuiApplication and gst::init.
    # Unknown unrelated options are ignored by the app, so cannot serve as a probe.
    result = subprocess.run(["nagametv", "--features=deb-package-probe"],
                            text=True, capture_output=True, timeout=15)
    if result.returncode != 2 or "deb-package-probe" not in result.stderr:
        raise RuntimeError(f"Argument parsing probe failed: {result.returncode}\n{result.stderr}")
    subprocess.run(["desktop-file-validate",
                    "/usr/share/applications/io.github.ouvill.nagametv.desktop"], check=True)
    print(f"Ubuntu {ubuntu.value}: {count} ELF files resolved; argument parsing and desktop entry OK")


if __name__ == "__main__":
    main()
