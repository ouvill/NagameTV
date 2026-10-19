"""Check the remote page and compare a temporary build with its embedded assets."""
from pathlib import Path
import shlex
import subprocess
import sys
import tempfile

from scripts.build.support import ROOT, ensure_lock, lock_fds

WEB = ROOT / "rust/crates/viewer-remote/web"
REBUILD = "Run npm run build in rust/crates/viewer-remote/web and commit dist/."


def asset_files(directory):
    if not directory.is_dir():
        raise ValueError(f"Missing asset directory: {directory}. {REBUILD}")
    files = {}
    for path in sorted(directory.rglob("*")):
        if path.is_symlink():
            raise ValueError(f"Asset must not be a symlink: {path}. {REBUILD}")
        if path.is_file():
            files[path.relative_to(directory).as_posix()] = path.read_bytes()
    return files


def compare_assets(committed, generated):
    expected, actual = asset_files(committed), asset_files(generated)
    differences = [name for name in sorted(expected.keys() | actual.keys())
                   if expected.get(name) != actual.get(name)]
    if differences:
        raise ValueError("Remote Web dist differs from the rebuilt assets: "
                         + ", ".join(differences) + f". {REBUILD}")
    if not actual:
        raise ValueError(f"Remote Web build produced no assets. {REBUILD}")
    return len(actual)


def check(web):
    if not (web / "node_modules").is_dir():
        raise ValueError(f"Missing {web / 'node_modules'}. Run npm ci in {web} before testing.")
    with tempfile.TemporaryDirectory(prefix="nagametv-remote-web-") as temporary:
        generated = Path(temporary) / "dist"
        for command in (["npm", "run", "check"], ["npm", "test"],
                        ["node", "build.mjs", "--out", str(generated)]):
            print(f"[{web}] {shlex.join(command)}", flush=True)
            subprocess.run(command, cwd=web, check=True, pass_fds=lock_fds())
        count = compare_assets(web / "dist", generated)
        print(f"Verified {count} remote Web assets match dist/ byte for byte", flush=True)


def main():
    ensure_lock()
    try:
        check(WEB)
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f"Remote Web check failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
