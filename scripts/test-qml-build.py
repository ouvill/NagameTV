#!/usr/bin/env python3
"""Exercise QML build scheduling without loading Qt or hardware."""
import json
import os
from pathlib import Path
import subprocess
import tempfile

from test_support import ROOT, ensure_lock, lock_fds


def main():
    ensure_lock()
    with tempfile.TemporaryDirectory(prefix="nagametv-qml-build-") as directory:
        fixture = Path(directory)
        (fixture / "Cargo.toml").write_text(
            '[package]\nname="qml-build-tests"\nversion="0.0.0"\nedition="2021"\n'
            '[lib]\npath="lib.rs"\n[dependencies]\njobserver="=0.1.35"\n')
        source = ROOT / "rust/vendor/qt-build-utils/src/parallel.rs"
        (fixture / "lib.rs").write_text(f'#[path={json.dumps(str(source))}] mod parallel;\n')
        subprocess.run(
            ["cargo", "test", "--offline", "--manifest-path", str(fixture / "Cargo.toml"),
             "--lib"], cwd=ROOT, env=os.environ, pass_fds=lock_fds(), check=True, timeout=90)


if __name__ == "__main__":
    main()
