#!/usr/bin/env python3
"""Hardware-free checks of build identity, incremental Cargo builds and packaging."""
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parent.parent
COLLECTOR = ROOT / "rust/build/build_info.rs"
spec = importlib.util.spec_from_file_location("build_source_info", ROOT / "scripts/build-source-info.py")
packaging = importlib.util.module_from_spec(spec)
spec.loader.exec_module(packaging)


class BuildInfoTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.env = {key: value for key, value in os.environ.items()
                    if not key.startswith(("NAGAMETV_BUILD_", "CARGO_", "GIT_"))}
        self.env.update(SOURCE_DATE_EPOCH="1700000000", GIT_AUTHOR_NAME="Build test",
                        GIT_AUTHOR_EMAIL="test@example.invalid", GIT_COMMITTER_NAME="Build test",
                        GIT_COMMITTER_EMAIL="test@example.invalid")

    def run_command(self, *args, **kwargs):
        return subprocess.check_output(args, cwd=self.root, env=self.env, text=True, **kwargs).strip()

    def git(self, *args):
        return self.run_command("git", *args)

    def prepare_repository(self):
        self.git("init", "--quiet", "--initial-branch=fixture")
        (self.root / ".gitignore").write_text("target/\n")
        (self.root / "rust").mkdir()
        (self.root / "rust/Cargo.toml").write_text(
            '[package]\nname="build-info-fixture"\nversion="1.2.3"\nedition="2024"\n'
            '[lib]\npath="lib.rs"\n[features]\ndistribution=[]\n')
        # Use the production generator and let Cargo itself exercise invalidation.
        (self.root / "rust/build.rs").write_text(
            f'#[path = {json.dumps(str(COLLECTOR))}] mod build_info;\n'
            'fn main() { build_info::generate().unwrap(); }\n')
        (self.root / "rust/lib.rs").write_text(
            'pub const BUILD: &str = include_str!(concat!(env!("OUT_DIR"), "/build_info.rs"));\n')
        self.run_command("cargo", "generate-lockfile", "--offline", "--manifest-path", "rust/Cargo.toml")
        self.git("add", ".")
        self.git("-c", "commit.gpgsign=false", "commit", "--quiet", "-m", "Fixture")

    def build(self, root=None):
        root = root or self.root
        output = self.run_command("cargo", "build", "--offline", "--locked", "--features", "distribution",
                                  "--manifest-path", str(root / "rust/Cargo.toml"), "--message-format=json")
        messages = [json.loads(line) for line in output.splitlines()]
        directory = next(message["out_dir"] for message in messages
                         if message["reason"] == "build-script-executed")
        return (Path(directory) / "build_info.rs").read_text()

    def test_incremental_git_state_and_linked_worktree(self):
        self.prepare_repository()
        commit = self.git("rev-parse", "HEAD")
        clean = self.build()
        self.assertIn(commit, clean)
        self.assertIn("WorktreeState::Clean", clean)
        self.assertIn('version: "1.2.3"', clean)
        self.assertIn("built_unix_seconds: 1700000000", clean)
        self.assertIn('features: &["DISTRIBUTION"]', clean)
        # No source file or index changes: addition and removal of an untracked file.
        untracked = self.root / "untracked.txt"
        untracked.write_text("change")
        self.assertIn("WorktreeState::Dirty", self.build())
        untracked.unlink()
        self.assertEqual(self.build(), clean)
        self.git("-c", "commit.gpgsign=false", "commit", "--allow-empty", "--quiet", "-m", "New HEAD")
        self.assertIn(self.git("rev-parse", "HEAD"), self.build())
        linked = self.root / "target/linked"
        self.git("worktree", "add", "--detach", str(linked), commit)
        self.assertEqual(self.build(linked), clean)
        # A source archive must not borrow the identity of an enclosing repository.
        archive = self.root / "target/archive"
        archive.mkdir()
        self.run_command("git", "archive", "HEAD", "--output", str(archive / "source.tar"))
        self.run_command("tar", "-xf", str(archive / "source.tar"), "-C", str(archive))
        self.assertIn("Source::Unavailable", self.build(archive))
        self.env["NAGAMETV_BUILD_SOURCE"] = f"{commit}:dirty"
        supplied = self.build(archive)
        self.assertIn(commit, supplied)
        self.assertIn("WorktreeState::Dirty", supplied)

    def test_generator_validation(self):
        executable = self.root / "build-info-tests"
        self.run_command("rustc", "--edition=2024", "--test", str(COLLECTOR), "-o", str(executable))
        self.run_command(str(executable))

    def test_flatpak_manifest_preserves_sources_and_identity(self):
        manifest_path = ROOT / "packaging/flatpak/io.github.ouvill.nagametv.json"
        identity = "a" * 40 + ":dirty"
        manifest = packaging.flatpak_manifest(manifest_path, identity)
        application = next(module for module in manifest["modules"] if module["name"] == "nagametv")
        self.assertEqual(application["build-options"]["env"]["NAGAMETV_BUILD_SOURCE"], identity)
        self.assertEqual(application["build-options"]["env"]["CARGO_HOME"], "/run/build/nagametv/cargo")
        for source in application["sources"]:
            path = source if isinstance(source, str) else source.get("path")
            if path:
                self.assertTrue(Path(path).is_absolute())
                self.assertTrue(Path(path).exists(), path)
        original = json.loads(manifest_path.read_text())
        self.assertEqual(manifest["finish-args"], original["finish-args"])


if __name__ == "__main__":
    unittest.main()
