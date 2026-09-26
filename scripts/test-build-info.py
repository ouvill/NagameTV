#!/usr/bin/env python3
"""Hardware-free checks of build identity, incremental Cargo builds and packaging."""
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parent.parent
COLLECTOR = ROOT / "rust/crates/viewer-build-info/build.rs"
APPLICATION = ROOT / "rust/build/build_info.rs"
spec = importlib.util.spec_from_file_location("build_source_info", ROOT / "scripts/build-source-info.py")
packaging = importlib.util.module_from_spec(spec)
spec.loader.exec_module(packaging)


class BuildInfoTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.env = {key: value for key, value in os.environ.items()
                    if not key.startswith(("NAGAMETV_BUILD_", "CARGO_", "GIT_"))
                    or key in ("CARGO_HOME", "CARGO_BUILD_JOBS")}
        self.env.update(SOURCE_DATE_EPOCH="1700000000", GIT_AUTHOR_NAME="Build test",
                        GIT_AUTHOR_EMAIL="test@example.invalid", GIT_COMMITTER_NAME="Build test",
                        GIT_COMMITTER_EMAIL="test@example.invalid",
                        CARGO_TARGET_DIR=str(self.root / "target"))

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
            '[[bin]]\nname="build-info-fixture"\npath="main.rs"\n'
            '[features]\ndistribution=[]\n'
            '[dependencies]\nserde_json="1.0"\n'
            'viewer-build-info={path="crates/viewer-build-info"}\n'
            'viewer-diagnostics={path="crates/viewer-diagnostics"}\n'
            '[build-dependencies]\nviewer-translations={path="crates/viewer-translations"}\n')
        for name in ("viewer-build-info", "viewer-diagnostics", "viewer-translations"):
            shutil.copytree(ROOT / "rust/crates" / name, self.root / "rust/crates" / name,
                            ignore=shutil.ignore_patterns("target", "Cargo.lock"))
        (self.root / "rust/build").mkdir()
        shutil.copy2(ROOT / "rust/build/translations.rs", self.root / "rust/build/translations.rs")
        (self.root / "translations").mkdir()
        shutil.copy2(ROOT / "translations/app_ja.ts", self.root / "translations/app_ja.ts")
        # Use both production generators and let Cargo exercise invalidation.
        # Count the application's build-script executions without requiring Qt.
        (self.root / "rust/build.rs").write_text(
            f'#[path = {json.dumps(str(APPLICATION))}] mod build_info;\n'
            'fn main() -> Result<(), Box<dyn std::error::Error>> {\n'
            '    use std::io::Write;\n'
            '    println!("cargo:rerun-if-changed=qt-input.txt");\n'
            # CxxQtBuilder watches both the .qrc and its contents. Reproduce that
            # boundary using the real lrelease outputs, without initializing Qt.
            '    let qrc = std::path::Path::new(viewer_translations::QRC_PATH);\n'
            '    println!("cargo:rerun-if-changed={}", qrc.display());\n'
            '    println!("cargo:rerun-if-changed={}", qrc.with_file_name("ja.qm").display());\n'
            '    let counter = std::path::PathBuf::from(std::env::var("OUT_DIR")?).join("runs");\n'
            '    writeln!(std::fs::OpenOptions::new().create(true).append(true).open(counter)?,\n'
            '             "{}", std::env::var("CARGO_MANIFEST_DIR")?)?;\n'
            '    build_info::generate()\n'
            '}\n')
        (self.root / "rust/qt-input.txt").write_text("initial\n")
        (self.root / "rust/main.rs").write_text(
            f'#[path = {json.dumps(str(ROOT / "rust/src/build_info.rs"))}] mod build_info;\n'
            'fn main() { println!("{}", build_info::json()); }\n')
        self.run_command("cargo", "generate-lockfile", "--offline", "--manifest-path", "rust/Cargo.toml")
        self.git("add", ".")
        self.git("-c", "commit.gpgsign=false", "commit", "--quiet", "-m", "Fixture")

    def build(self, root=None, *, features="distribution", native_builds=1, fresh=None):
        root = root or self.root
        # Each source tree gets its own build-script outputs; dependency reuse
        # across worktrees is Cargo's responsibility, outside this assertion.
        self.env["CARGO_TARGET_DIR"] = str(root / "target")
        if (root / ".git").exists():
            self.env["NAGAMETV_BUILD_FINGERPRINT"] = (
                packaging.source_identity(root, env=self.env) + ":" + packaging.source_fingerprint(root))
            self.env["NAGAMETV_BUILD_SNAPSHOT_ROOT"] = str(root)
        else:
            self.env.pop("NAGAMETV_BUILD_FINGERPRINT", None)
            self.env.pop("NAGAMETV_BUILD_SNAPSHOT_ROOT", None)
        output = self.run_command("cargo", "build", "--offline", "--locked", "--features", features,
                                  "--manifest-path", str(root / "rust/Cargo.toml"), "--message-format=json")
        messages = [json.loads(line) for line in output.splitlines()]
        directory = next(message["out_dir"] for message in messages
                         if message["reason"] == "build-script-executed"
                         and message["package_id"].endswith("#build-info-fixture@1.2.3"))
        runs = (Path(directory) / "runs").read_text().splitlines()
        self.assertEqual(runs.count(str(root / "rust")), native_builds,
                         "source identity changes must not rerun the application's build script")
        binary = next(message["executable"] for message in messages
                      if message["reason"] == "compiler-artifact" and message.get("executable")
                      and message["target"]["name"] == "build-info-fixture")
        if fresh is not None:
            artifact = next(message for message in messages
                            if message["reason"] == "compiler-artifact" and message.get("executable")
                            and message["target"]["name"] == "build-info-fixture")
            self.assertEqual(artifact["fresh"], fresh, "unchanged builds must reuse the executable")
        return json.loads(self.run_command(binary))

    def test_incremental_git_state_and_linked_worktree(self):
        self.prepare_repository()
        commit = self.git("rev-parse", "HEAD")
        clean = self.build()
        self.assertEqual(clean["source"], {"kind": "git", "commit": commit, "worktree": "clean"})
        self.assertEqual(clean["version"], "1.2.3")
        self.assertEqual(clean["built_unix_seconds"], 1700000000)
        self.assertEqual(clean["features"], ["DISTRIBUTION"])
        self.assertEqual(clean["profile"], "debug")
        self.assertEqual(clean["rustc"], self.run_command("rustc", "--version"))
        self.assertTrue(clean["target"])
        self.assertEqual(self.build(fresh=True), clean)
        # No source file or index changes: addition and removal of an untracked file.
        untracked = self.root / "untracked.txt"
        untracked.write_text("change")
        self.assertEqual(self.build()["source"]["worktree"], "dirty")
        untracked.unlink()
        self.assertEqual(self.build(), clean)
        self.git("-c", "commit.gpgsign=false", "commit", "--allow-empty", "--quiet", "-m", "New HEAD")
        self.assertEqual(self.build()["source"]["commit"], self.git("rev-parse", "HEAD"))
        self.assertEqual(self.build(features="")["features"], [])
        # Actual native inputs still invalidate the application build script.
        (self.root / "rust/qt-input.txt").write_text("changed\n")
        self.assertEqual(self.build(native_builds=2)["features"], ["DISTRIBUTION"])
        catalog = self.root / "translations/app_ja.ts"
        catalog.write_text(catalog.read_text() + "\n")
        self.assertEqual(self.build(native_builds=3)["features"], ["DISTRIBUTION"])
        self.assertEqual(self.build(native_builds=3)["features"], ["DISTRIBUTION"])
        linked = self.root / "target/linked"
        self.git("worktree", "add", "--detach", str(linked), commit)
        self.assertEqual(self.build(linked), clean)
        # A source archive must not borrow the identity of an enclosing repository.
        archive = self.root / "target/archive"
        archive.mkdir()
        self.env.pop("NAGAMETV_BUILD_SOURCE", None)
        self.run_command("git", "archive", "HEAD", "--output", str(archive / "source.tar"))
        self.run_command("tar", "-xf", str(archive / "source.tar"), "-C", str(archive))
        self.assertEqual(self.build(archive)["source"], {"kind": "unavailable"})
        self.env["NAGAMETV_BUILD_SOURCE"] = f"{commit}:dirty"
        supplied = self.build(archive)
        self.assertEqual(supplied["source"], {"kind": "git", "commit": commit, "worktree": "dirty"})
        self.env["SOURCE_DATE_EPOCH"] = "1700000123"
        self.assertEqual(self.build(archive)["built_unix_seconds"], 1700000123)

    def test_source_digest_tracks_contents_deletions_links_and_submodules(self):
        self.prepare_repository()
        original = packaging.source_fingerprint(self.root)
        # A touched file has the same contents; ignored outputs are not inputs.
        source = self.root / "rust/main.rs"
        source.touch()
        (self.root / "target").mkdir(exist_ok=True)
        (self.root / "target/output").write_text("ignored")
        self.assertEqual(packaging.source_fingerprint(self.root), original)
        source.write_text(source.read_text() + "\n")
        modified = packaging.source_fingerprint(self.root)
        self.assertNotEqual(modified, original)
        source.unlink()
        self.assertNotEqual(packaging.source_fingerprint(self.root), modified)
        link = self.root / "link"
        link.symlink_to("missing-a")
        linked = packaging.source_fingerprint(self.root)
        link.unlink()
        link.symlink_to("missing-b")
        self.assertNotEqual(packaging.source_fingerprint(self.root), linked)
        # Use a separate local Git repository, without any network access.
        sub = self.root / "target/sub-origin"
        sub.mkdir()
        self.git("-C", str(sub), "init", "--quiet")
        (sub / "input").write_text("one")
        self.git("-C", str(sub), "add", ".")
        self.git("-C", str(sub), "-c", "commit.gpgsign=false", "commit", "--quiet", "-m", "Initial")
        self.git("-c", "protocol.file.allow=always", "submodule", "add", "--quiet", str(sub), "sub")
        before = packaging.source_fingerprint(self.root)
        (self.root / "sub/input").write_text("two")
        self.assertNotEqual(packaging.source_fingerprint(self.root), before)

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
        local_crates = {source["dest"] for source in application["sources"] if isinstance(source, dict)
                        and source.get("type") == "dir"}
        self.assertIn("rust/crates/viewer-build-info", local_crates)
        self.assertIn("rust/crates/viewer-translations", local_crates)
        original = json.loads(manifest_path.read_text())
        self.assertEqual(manifest["finish-args"], original["finish-args"])


if __name__ == "__main__":
    unittest.main()
