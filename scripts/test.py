#!/usr/bin/env python3
"""Run selected checks under one build/test lock; default: hardware-free CPU suites."""
import argparse
from dataclasses import dataclass
from enum import StrEnum
import json
import os
from pathlib import Path
import shlex
import signal
import subprocess
import sys
import tempfile
import time

# This stable file entry point also works when invoked outside the repository.
sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from scripts.build.support import ROOT, build_environment, ensure_lock, lock_fds
from scripts.testing.suites import SUITES, Requirement

CRATES = ("viewer-comments", "viewer-epg-events", "viewer-diagnostics", "viewer-remote", "tsreadex", "viewer-mpegts", "arib-b24", "viewer-web-bml")
RUST = ("app", *CRATES)
NATIVE = tuple(name for name, suite in SUITES.items() if suite.requirement == Requirement.CPU)
GUI = tuple(name for name, suite in SUITES.items() if suite.requirement == Requirement.GUI
            and name != "ui-capture")
GROUPS = {"rust": RUST, "native": NATIVE, "gui": GUI,
          "cpu": ("checks", "web-bml-adapter", *RUST, *NATIVE), "all": ("checks", "web-bml-adapter", *RUST, *NATIVE, *GUI)}


class Status(StrEnum):
    NOT_RUN = "not-run"
    PASSED = "passed"
    FAILED = "failed"
    UNAVAILABLE = "environment-unavailable"


@dataclass
class Step:
    name: str
    command: list[str]
    output: Path | None = None
    environment_check: bool = False
    status: Status = Status.NOT_RUN
    seconds: float = 0
    returncode: int | None = None

    def run(self, directory, env):
        print(f"[{self.name}] {shlex.join(self.command)}", flush=True)
        start = time.monotonic()
        with (directory / f"{self.name}.log").open("w") as log:
            if self.output:
                with self.output.open("w") as output:
                    self.returncode = self.execute(env, output, log)
            else:
                self.returncode = self.execute(env, log, subprocess.STDOUT)
        self.seconds = round(time.monotonic() - start, 2)
        self.status = (Status.PASSED if self.returncode == 0 else
                       Status.UNAVAILABLE if self.environment_check else Status.FAILED)
        print(f"[{self.name}] {self.status} ({self.seconds}s)", flush=True)
        if self.returncode:
            print((directory / f"{self.name}.log").read_text()[-12000:], file=sys.stderr)
        return self.returncode == 0

    def execute(self, env, stdout, stderr):
        process = subprocess.Popen(self.command, cwd=ROOT, env=env, pass_fds=lock_fds(),
                                   stdout=stdout, stderr=stderr, start_new_session=True)
        try:
            return process.wait()
        except BaseException:
            # Cancel the whole build/test, including Cargo's compiler children.
            # A process exiting between detection and signaling is a normal race.
            try:
                os.killpg(process.pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                pass
            finally:
                # The leader may exit before descendants which ignored SIGTERM.
                try:
                    os.killpg(process.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                process.wait()
            raise


def plan(suites, directory, args):
    build, checks, tests = [], [], []
    if "web-bml-adapter" in suites:
        tests.append(Step("web-bml-adapter", ["node", "--test", "tests/web-bml/adapter.test.mjs"]))
    if "checks" in suites:
        checks = [Step("format", ["cargo", "fmt", "--manifest-path", "rust/Cargo.toml", "--check"])]
        for module in ("packaging.check_release_metadata", "testing.check_ui_style",
                       "packaging.flatpak_cargo_sources"):
            checks.append(Step(module.rsplit(".", 1)[-1], [sys.executable, "-m", f"scripts.{module}",
                               *(["--check"] if module.endswith("flatpak_cargo_sources") else [])]))
        checks.append(Step("comment-sql", [sys.executable, "-m", "scripts.testing.check_comment_sql"]))
    if "checks" in suites or "tooling" in suites:
        checks.insert(0, Step("tooling", [sys.executable, "-m", "unittest", "discover",
                                         "-s", "tests/tooling", "-p", "test_*.py"]))
    if "checks" in suites or "localization" in suites:
        checks.append(Step("localization-catalog", [sys.executable, "-m", "scripts.testing.check_localization"]))
    if any(suite in RUST for suite in suites):
        checks.insert(0, Step("nextest-version", ["cargo", "nextest", "--version"]))
    for suite in suites:
        if suite not in RUST:
            continue
        manifest = "rust/Cargo.toml" if suite == "app" else f"rust/crates/{suite}/Cargo.toml"
        features = ["--features", "network"] if suite in CRATES[:2] else []
        cargo_args = ["--manifest-path", manifest, "--locked", *features]
        metadata = directory / f"{suite}-binaries.json"
        cargo_metadata = directory / f"{suite}-cargo.json"
        build.append(Step(f"metadata-{suite}", ["cargo", "metadata", *cargo_args,
                                               "--format-version", "1"], output=cargo_metadata))
        build.append(Step(f"build-{suite}", ["cargo", "nextest", "list", *cargo_args,
            "--cargo-profile", args.profile, "--config-file", str(ROOT / ".config/nextest.toml"),
            "--message-format", "json", "--list-type", "binaries-only"], output=metadata))
        command = ["cargo", "nextest", "run", "--cargo-metadata", str(cargo_metadata),
                   "--binaries-metadata", str(metadata),
                   "--config-file", str(ROOT / ".config/nextest.toml")]
        if args.test_threads:
            command.extend(["--test-threads", str(args.test_threads)])
        if args.filter:
            command.extend(["-E", args.filter])
        tests.append(Step(suite, command))
        # nextest does not execute rustdoc examples. Preserve library doctests.
        if suite in CRATES and not args.filter:
            tests.append(Step(f"doc-{suite}", ["cargo", "test", *cargo_args,
                                              "--profile", args.profile, "--doc"]))
    if any(suite in SUITES for suite in suites):
        build.append(Step("build-qt", [sys.executable, "-m", "scripts.testing.binary",
                                      "--prepare", str(directory / "nagametv"),
                                      *(["--evaluation-legacy-comments"] if
                                        args.suite_args == ["--evaluation-legacy-comments"] else [])]))
    if "checks" in suites and "app" in suites:
        tests.insert(0, Step("qml-lint", ["bash", "scripts/testing/check-qml.sh"]))
    for suite in suites:
        if suite not in SUITES:
            continue
        command = [sys.executable, "-m", "scripts.testing.suites", suite, *args.suite_args]
        if SUITES[suite].requirement == Requirement.GUI:
            command = [sys.executable, "-m", "scripts.testing.gui_session", "--", *command]
        tests.append(Step(suite, command))
    if "connection" in suites:
        tests.append(Step("cli", [sys.executable, "-m", "scripts.testing.check_cli", str(directory / "nagametv")]))
    if any(suite in SUITES and SUITES[suite].requirement == Requirement.GUI for suite in suites):
        # A missing required resource stops the run before dependent builds/tests.
        checks.insert(0, Step("gui-environment", [sys.executable, "-m", "scripts.testing.gui_session", "--check"],
                              environment_check=True))
    return [*checks, *build, *tests]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("suites", nargs="*", help="cpu, rust, native, gui, all, or individual suite names")
    parser.add_argument("--list", action="store_true", help="list suites without building or accessing hardware")
    parser.add_argument("--profile", choices=("dev", "release"), default="dev")
    parser.add_argument("--log-dir", type=Path, default=ROOT / "build/test-runs",
                        help="parent directory for per-run logs and binaries")
    parser.add_argument("--test-threads", type=int, help="nextest concurrency (default: 2)")
    parser.add_argument("--filter", help="nextest filterset; only allowed with Rust suites")
    arguments = sys.argv[1:]
    delimiter = arguments.index("--") if "--" in arguments else len(arguments)
    args = parser.parse_args(arguments[:delimiter])
    args.suite_args = arguments[delimiter + 1:]
    if args.list:
        for group, members in GROUPS.items():
            print(f"{group}: {', '.join(members)}")
        print("web-bml-adapter: hardware-free browser adapter state and protocol tests (Node.js)")
        print("tooling: hardware-free tests of scripts (also included in checks)")
        print("ui-capture: manual GUI images for review (explicit selection only)")
        return 0
    suites = []
    for name in args.suites or ["cpu"]:
        if name not in (*GROUPS, "checks", "tooling", "web-bml-adapter", *RUST, *SUITES):
            parser.error(f"unknown suite: {name}; use --list")
        suites.extend(suite for suite in GROUPS.get(name, (name,)) if suite not in suites)
    if args.suite_args:
        if len(args.suites) != 1 or args.suites[0] not in SUITES:
            parser.error("arguments after -- require one explicit Qt suite")
        from scripts.testing.suites import validate_arguments
        try:
            validate_arguments(args.suites[0], args.suite_args)
        except ValueError as error:
            parser.error(str(error))
    if args.filter and any(suite not in RUST for suite in suites):
        parser.error("--filter requires explicit Rust suites (for example: app --filter 'test(settings::)')")
    if args.test_threads is not None and args.test_threads < 1:
        parser.error("--test-threads must be positive")
    ensure_lock()
    os.chdir(ROOT)
    env = build_environment(args.profile)
    log_root = args.log_dir.resolve()
    log_root.mkdir(parents=True, exist_ok=True)
    directory = Path(tempfile.mkdtemp(prefix="run-", dir=log_root))
    env["NAGAMETV_TEST_BINARY"] = str(directory / "nagametv")
    steps = plan(suites, directory, args)
    print(f"Test logs and summary: {directory}", flush=True)
    current = None
    try:
        for current in steps:
            if not current.run(directory, env):
                break
    except (OSError, KeyboardInterrupt) as error:
        if current:
            current.status = Status.FAILED
        print(f"Test run stopped: {error}", file=sys.stderr)
    finally:
        report = {"suites": suites, "profile": args.profile, "steps": [
            {"name": step.name, "status": step.status, "seconds": step.seconds,
             "returncode": step.returncode, "log": f"{step.name}.log"} for step in steps]}
        (directory / "summary.json").write_text(json.dumps(report, indent=2) + "\n")
        for step in steps:
            print(f"  {step.status:24} {step.name}", flush=True)
        print(f"Summary: {directory / 'summary.json'}", flush=True)
    return 0 if all(step.status == Status.PASSED for step in steps) else 1


if __name__ == "__main__":
    def interrupted(*_):
        raise KeyboardInterrupt("received SIGTERM")
    signal.signal(signal.SIGTERM, interrupted)
    sys.exit(main())
