"""Hardware-free regressions for the remote page's committed asset check."""
from contextlib import redirect_stdout
import io
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

from scripts.testing import check_remote_web as web_check


class RemoteWebTests(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.web = Path(directory.name) / "web"
        self.web.mkdir()
        self.committed = self.web / "dist"
        self.generated = Path(directory.name) / "generated"
        self.write_assets(self.committed)
        self.write_assets(self.generated)

    def write_assets(self, directory):
        (directory / "assets").mkdir(parents=True)
        (directory / "index.html").write_bytes(b"<html>remote</html>")
        (directory / "assets/app.js").write_bytes(b"remote();")

    def check(self):
        with redirect_stdout(io.StringIO()):
            web_check.check(self.web)

    def test_missing_dependencies_fail_before_any_process_or_network_access(self):
        with patch.object(web_check.subprocess, "run") as run:
            with self.assertRaisesRegex(ValueError, "node_modules.*npm ci"):
                self.check()
        run.assert_not_called()

    def test_rebuild_is_temporary_and_preserves_committed_assets(self):
        (self.web / "node_modules").mkdir()
        before = web_check.asset_files(self.committed)
        outputs = []

        def run(command, **kwargs):
            self.assertEqual(kwargs["cwd"], self.web)
            if command[0] == "node":
                generated = Path(command[-1])
                self.assertEqual(command[1:3], ["build.mjs", "--out"])
                self.assertNotEqual(generated, self.committed)
                self.write_assets(generated)
                outputs.append(generated)

        with patch.object(web_check.subprocess, "run", side_effect=run) as run:
            self.check()
        self.assertEqual([call.args[0] for call in run.call_args_list[:2]],
                         [["npm", "run", "check"], ["npm", "test"]])
        self.assertEqual(len(outputs), 1)
        self.assertFalse(outputs[0].exists())
        self.assertEqual(web_check.asset_files(self.committed), before)

    def test_command_failure_stops_later_checks_and_preserves_assets(self):
        (self.web / "node_modules").mkdir()
        before = web_check.asset_files(self.committed)
        for failure in range(3):
            with self.subTest(failure=failure):
                error = subprocess.CalledProcessError(7, "fixture failure")
                with patch.object(web_check.subprocess, "run",
                                  side_effect=[None] * failure + [error]) as run:
                    with self.assertRaises(subprocess.CalledProcessError):
                        self.check()
                self.assertEqual(run.call_count, failure + 1)
                self.assertEqual(web_check.asset_files(self.committed), before)

    def test_same_size_and_mtime_do_not_hide_changed_bytes(self):
        expected = self.committed / "assets/app.js"
        actual = self.generated / "assets/app.js"
        actual.write_bytes(b"broken();")
        stat = expected.stat()
        os.utime(actual, ns=(stat.st_atime_ns, stat.st_mtime_ns))
        with self.assertRaisesRegex(ValueError, r"assets/app.js.*npm run build.*commit dist/"):
            web_check.compare_assets(self.committed, self.generated)

    def test_extra_and_missing_files_are_reported(self):
        (self.generated / "assets/app.js").unlink()
        (self.generated / "assets/new.js").write_bytes(b"new();")
        with self.assertRaisesRegex(ValueError, r"assets/app.js, assets/new.js.*npm run build"):
            web_check.compare_assets(self.committed, self.generated)

    def test_missing_or_empty_output_cannot_pass(self):
        with self.assertRaisesRegex(ValueError, "Missing asset directory.*npm run build"):
            web_check.compare_assets(self.committed, self.generated / "missing")
        empty = self.generated / "empty"
        empty.mkdir()
        with self.assertRaisesRegex(ValueError, "no assets.*npm run build"):
            web_check.compare_assets(empty, empty)


if __name__ == "__main__":
    unittest.main()
