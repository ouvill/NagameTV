#!/usr/bin/env python3
"""Hardware-free contract tests for cross-process exclusion and runner failures."""
import json
import os
from pathlib import Path
import selectors
import subprocess
import sys
import tempfile
import time
import unittest

ROOT = Path(__file__).resolve().parent.parent
LOCK = ROOT / "scripts/with-build-lock.sh"


class RunnerTests(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.directory = Path(directory.name)
        self.env = dict(os.environ, NAGAMETV_BUILD_LOCK_DIR=str(self.directory / "lock"))
        self.env.pop("NAGAMETV_BUILD_LOCK_HELD", None)
        self.env.pop("NAGAMETV_TEST_BINARY", None)

    def start(self, command, **kwargs):
        process = subprocess.Popen(command, env=self.env, stdin=subprocess.PIPE,
                                   stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, **kwargs)
        def cleanup():
            if process.poll() is None:
                process.kill()
            process.communicate(timeout=10)
        self.addCleanup(cleanup)
        return process

    def line(self, stream):
        with selectors.DefaultSelector() as selector:
            selector.register(stream, selectors.EVENT_READ)
            self.assertTrue(selector.select(10), "child did not reach the synchronization point")
        return stream.readline().strip()

    def test_different_worktrees_wait_and_inherited_lock_is_reentrant(self):
        first_tree = self.directory / "first"
        second_tree = self.directory / "second"
        first_tree.mkdir()
        second_tree.mkdir()
        first = self.start(["bash", str(LOCK), sys.executable, "-c",
                            "print('holding', flush=True); input()"], cwd=first_tree)
        self.assertEqual(self.line(first.stdout), "holding")
        second = self.start(["bash", str(LOCK), "bash", str(LOCK),
                             sys.executable, "-c", "print('acquired')"], cwd=second_tree)
        self.assertIn("Waiting for another", self.line(second.stderr))
        first.stdin.write("release\n")
        first.stdin.flush()
        self.assertEqual(self.line(second.stdout), "acquired")
        self.assertEqual(first.wait(timeout=10), 0)
        self.assertEqual(second.wait(timeout=10), 0)

    def test_stale_marker_does_not_bypass_lock_and_failure_releases_it(self):
        first = self.start(["bash", str(LOCK), sys.executable, "-c",
                            "print('holding', flush=True); input(); raise SystemExit(7)"])
        self.assertEqual(self.line(first.stdout), "holding")
        self.env["NAGAMETV_BUILD_LOCK_HELD"] = str(self.directory / "lock/lock")
        second = self.start(["bash", str(LOCK), sys.executable, "-c", "print('acquired')"])
        self.assertIn("Waiting for another", self.line(second.stderr))
        first.stdin.write("fail\n")
        first.stdin.flush()
        self.assertEqual(first.wait(timeout=10), 7)
        self.assertEqual(self.line(second.stdout), "acquired")
        self.assertEqual(second.wait(timeout=10), 0)

    def fake_cargo(self, body):
        executable = self.directory / "cargo"
        executable.write_text(f"#!{sys.executable}\n" + body)
        executable.chmod(0o755)
        self.env["PATH"] = str(self.directory) + os.pathsep + self.env["PATH"]
        self.env["RUNNER_FIXTURE"] = str(self.directory)

    def run_suite(self, suite):
        result = subprocess.run([sys.executable, str(ROOT / "scripts/test.py"), suite,
                                 "--log-dir", str(self.directory / "reports")],
                                env=self.env, text=True, capture_output=True, timeout=30)
        lines = [line for line in result.stdout.splitlines() if line.startswith("Summary: ")]
        self.assertEqual(len(lines), 1, result.stdout + result.stderr)
        return result, json.loads(Path(lines[0].removeprefix("Summary: ")).read_text())

    def test_nextest_failure_stops_run_and_reports_unexecuted_doctests(self):
        self.fake_cargo("""import json, sys
args = sys.argv[1:]
if args[0] == 'metadata' or args[:2] == ['nextest', 'list']:
    print('{}')
elif args[:2] == ['nextest', 'run']:
    assert '--binaries-metadata' in args
    assert json.load(open(args[args.index('--cargo-metadata') + 1])) == {}
    print('fixture assertion failed', file=sys.stderr)
    sys.exit(100)
""")
        result, report = self.run_suite("viewer-diagnostics")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("fixture assertion failed", result.stderr)
        statuses = {step["name"]: step["status"] for step in report["steps"]}
        self.assertEqual(statuses["metadata-viewer-diagnostics"], "passed")
        self.assertEqual(statuses["build-viewer-diagnostics"], "passed")
        self.assertEqual(statuses["viewer-diagnostics"], "failed")
        self.assertEqual(statuses["doc-viewer-diagnostics"], "not-run")

    def test_localization_builds_once_and_runs_two_private_binary_processes(self):
        binary = self.directory / "fixture-binary"
        binary.write_text(f"#!{sys.executable}\n" + """import os, pathlib, sys
with (pathlib.Path(os.environ['RUNNER_FIXTURE']) / 'runs').open('a') as output:
    output.write(sys.executable + ' ' + sys.argv[0] + ' ' + sys.argv[-1] + '\\n')
""")
        binary.chmod(0o755)
        self.fake_cargo("""import json, os, pathlib, sys
directory = pathlib.Path(os.environ['RUNNER_FIXTURE'])
assert sys.argv[1] == 'build'
with (directory / 'builds').open('a') as output:
    output.write('build\\n')
print(json.dumps({'reason': 'compiler-artifact', 'target': {'name': 'nagametv'},
                  'executable': str(directory / 'fixture-binary')}))
""")
        result, report = self.run_suite("localization")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertTrue(all(step["status"] == "passed" for step in report["steps"]))
        self.assertEqual((self.directory / "builds").read_text(), "build\n")
        runs = (self.directory / "runs").read_text().splitlines()
        self.assertEqual(len(runs), 2)
        self.assertTrue(runs[0].endswith(" localization"))
        self.assertTrue(runs[1].endswith(" missing-catalog"))
        self.assertTrue(all(str(self.directory / "reports/run-") in run for run in runs))

    def test_termination_stops_build_process_and_records_remaining_tests(self):
        self.fake_cargo("""import json, os, pathlib, subprocess, sys, time
if sys.argv[1:3] == ['nextest', 'list']:
    child = subprocess.Popen([sys.executable, '-c',
        "import signal,time; signal.signal(signal.SIGTERM, signal.SIG_IGN); "
        "print('ready', flush=True); time.sleep(60)"], stdout=subprocess.PIPE, text=True)
    assert child.stdout.readline().strip() == 'ready'
    directory = pathlib.Path(os.environ['RUNNER_FIXTURE'])
    (directory / 'pid.tmp').write_text(json.dumps([os.getpid(), child.pid]))
    (directory / 'pid.tmp').rename(directory / 'pid')
    time.sleep(60)
""")
        runner = self.start([sys.executable, str(ROOT / "scripts/test.py"), "viewer-diagnostics",
                             "--log-dir", str(self.directory / "reports")])
        pid_file = self.directory / "pid"
        deadline = time.monotonic() + 10
        while not pid_file.exists():
            self.assertLess(time.monotonic(), deadline, "build did not start")
            time.sleep(0.01)
        build_pid, child_pid = json.loads(pid_file.read_text())
        runner.terminate()
        stdout, stderr = runner.communicate(timeout=10)
        self.assertNotEqual(runner.returncode, 0, stderr)
        summary = next(line.removeprefix("Summary: ") for line in stdout.splitlines()
                       if line.startswith("Summary: "))
        statuses = {step["name"]: step["status"] for step in json.loads(Path(summary).read_text())["steps"]}
        self.assertEqual(statuses["build-viewer-diagnostics"], "failed")
        self.assertEqual(statuses["viewer-diagnostics"], "not-run")
        with self.assertRaises(ProcessLookupError):
            os.kill(build_pid, 0)
        child_stat = Path(f"/proc/{child_pid}/stat")
        deadline = time.monotonic() + 10
        while True:
            try:
                state = child_stat.read_text().split()[2]
            except FileNotFoundError:
                break  # Normal exit: the init process already reaped the child.
            if state == "Z":
                break
            self.assertLess(time.monotonic(), deadline, "compiler child is still running")
            time.sleep(0.01)


if __name__ == "__main__":
    unittest.main()
