#!/usr/bin/env python3
"""Hardware-free subprocess checks of the application's startup arguments."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile
import tomllib


def cli_environment(directory):
    env = dict(os.environ)
    for variable in ("XDG_CONFIG_HOME", "XDG_CACHE_HOME", "XDG_STATE_HOME"):
        env[variable] = str(Path(directory) / variable)
    # Tripwire: these commands must finish before Qt initialization. This
    # invalid platform never supplies or substitutes a display backend.
    env["QT_QPA_PLATFORM"] = "nagametv-invalid-platform"
    for variable in ("DISPLAY", "WAYLAND_DISPLAY", "NAGAMETV_DATA_DIR"):
        env.pop(variable, None)
    return env


def invoke(binary, arguments, data_directory="invalid-relative-path", log="info"):
    with tempfile.TemporaryDirectory(prefix="nagametv-cli-") as directory:
        env = dict(cli_environment(directory), NAGAMETV_DATA_DIR=data_directory, RUST_LOG=log)
        result = subprocess.run([str(binary), *arguments], cwd=directory, env=env, capture_output=True,
                                text=True, timeout=10)
        if any(Path(directory).iterdir()):
            raise AssertionError(f"CLI created application data: {arguments}")
        return result


def check_appliance_import(binary):
    example = Path(__file__).resolve().parents[2] / "packaging/pi-appliance/nagametv-connection.toml.example"
    with tempfile.TemporaryDirectory(prefix="nagametv-cli-") as directory:
        env = dict(cli_environment(directory), RUST_LOG="info")
        path = Path(env["XDG_CONFIG_HOME"]) / "nagametv/settings.toml"

        def import_seed(text):
            return subprocess.run([binary, "--import-connection"], input=text, cwd=directory,
                                  env=env, capture_output=True, text=True, timeout=10)

        result = import_seed("server = 'file:///invalid'")
        assert result.returncode == 1 and "Invalid server URL" in result.stderr, result.stderr
        assert not path.exists(), "invalid seed created preferences"
        result = import_seed(example.read_text())
        assert result.returncode == 0, result.stderr
        installed = path.read_text()
        preferences = tomllib.loads(installed)
        assert preferences["server"] == tomllib.loads(example.read_text())["server"]
        assert preferences["autoplay"] is True
        assert path.stat().st_uid == os.getuid()
        result = import_seed('server = "http://tuner"\nepgstation_server = false')
        assert result.returncode == 1 and "Could not import appliance connection" in result.stderr, result.stderr
        assert path.read_text() == installed, "invalid seed replaced preferences"
        malformed = 'server = "http://user:redact-test-secret@tuner\n'
        path.write_text(malformed)
        result = import_seed(example.read_text())
        assert result.returncode == 1 and "contain invalid TOML" in result.stderr, result.stderr
        assert "redact-test-secret" not in result.stderr, "parser logged a credential"
        assert path.read_text() == malformed, "import replaced malformed existing preferences"


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
                      ["--features=none", "--features=comments"],
                      ["--import-connection", "--kiosk"], ["--import-connection", "--build-info"],
                      ["--features=screen-navigation,screen-navigation"]):
        result = invoke(binary, arguments)
        assert result.returncode == 2, (arguments, result.returncode, result.stderr)
        assert not result.stdout and "error:" in result.stderr, (arguments, result)
    invalid_directories = [("", "info"), ("invalid-relative-path", "off"), (str(binary), "info")]
    if os.name == "posix" and Path("/proc/self").is_dir():
        invalid_directories.append(("/proc/self/nagametv-cli-data", "info"))
    for value, log in invalid_directories:
        result = invoke(binary, [], value, log)
        assert result.returncode == 1, (value, result.returncode, result.stderr)
        assert not result.stdout and "NAGAMETV_DATA_DIR" in result.stderr, (value, result)
        assert ("must be an absolute path" if not Path(value).is_absolute()
                else "Could not prepare writable directory") in result.stderr, (value, result)
    check_appliance_import(binary)
    print("CLI checks passed: information, argument errors, data directories and appliance import")


if __name__ == "__main__":
    main()
