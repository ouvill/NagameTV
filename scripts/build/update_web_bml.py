#!/usr/bin/env python3
"""Rebuild the checked-in browser-only web-bml bundle at a pinned upstream commit."""

from pathlib import Path
import shutil
import subprocess


ROOT = Path(__file__).resolve().parents[2]
UPSTREAM = ROOT / "build/web-bml-upstream"
ASSETS = ROOT / "assets/web-bml"
COMMIT = "dfc63a926f990e8fead9aaa0004072a462291d43"


def run(*args: str, cwd: Path = ROOT) -> None:
    subprocess.run(args, cwd=cwd, check=True)


def main() -> None:
    UPSTREAM.parent.mkdir(parents=True, exist_ok=True)
    if not (UPSTREAM / ".git").is_dir():
        run("git", "clone", "https://github.com/otya128/web-bml.git", str(UPSTREAM))
    run("git", "checkout", COMMIT, cwd=UPSTREAM)
    run("npm", "ci", "--ignore-scripts", "--no-audit", cwd=UPSTREAM)
    run("npm", "run", "build", cwd=UPSTREAM)
    config = UPSTREAM / "webpack.nagame.js"
    config.write_text(
        "const path = require('path');\n"
        "module.exports = {\n"
        "  mode: 'production',\n"
        "  entry: path.resolve(__dirname, '../../assets/web-bml/entry.js'),\n"
        "  output: { path: path.resolve(__dirname, '../../assets/web-bml'), filename: 'bundle.js' },\n"
        "  resolve: { alias: { 'web-bml$': path.resolve(__dirname, 'dist/client/bml_browser.js') } },\n"
        "  optimization: { minimize: true },\n"
        "};\n"
    )
    run(str(UPSTREAM / "node_modules/.bin/webpack"), "--config", str(config), cwd=UPSTREAM)
    shutil.copyfile(UPSTREAM / "LICENSE", ASSETS / "LICENSE-web-bml.txt")


if __name__ == "__main__":
    main()
