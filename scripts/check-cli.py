#!/usr/bin/env python3
"""Hardware-free subprocess checks of the application's startup arguments."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile


def invoke(binary, arguments):
    with tempfile.TemporaryDirectory(prefix="nagametv-cli-") as directory:
        env = dict(os.environ)
        for variable in ("XDG_CONFIG_HOME", "XDG_CACHE_HOME", "XDG_STATE_HOME"):
            env[variable] = str(Path(directory) / variable)
        # Tripwire: these commands must finish before Qt initialization. This
        # invalid platform never supplies or substitutes a display backend.
        env["QT_QPA_PLATFORM"] = "nagametv-invalid-platform"
        env.pop("DISPLAY", None)
        env.pop("WAYLAND_DISPLAY", None)
        result = subprocess.run([str(binary), *arguments], env=env, capture_output=True,
                                text=True, timeout=10)
        if any(Path(directory).iterdir()):
            raise AssertionError(f"CLI created application data: {arguments}")
        return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path, help="Built nagametv executable")
    binary = parser.parse_args().binary.resolve(strict=True)
    for flag in ("--help", "-h", "--version", "-V", "--build-info"):
        result = invoke(binary, [flag])
        assert result.returncode == 0, (flag, result.stderr)
        assert result.stdout and not result.stderr, (flag, result.stdout, result.stderr)
        if flag == "--build-info":
            json.loads(result.stdout)
    for arguments in (["--unknown"], ["--features=none,epg"], ["--features=epg,epg"],
                      ["--features=none", "--features=comments"]):
        result = invoke(binary, arguments)
        assert result.returncode == 2, (arguments, result.returncode, result.stderr)
        assert not result.stdout and "error:" in result.stderr, (arguments, result)
    print("CLI checks passed: information and errors finish before GUI initialization")


if __name__ == "__main__":
    main()
