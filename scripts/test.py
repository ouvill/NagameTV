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

from test_support import ROOT, build_environment, ensure_lock, lock_fds

CRATES = ("viewer-comments", "viewer-epg-events", "viewer-diagnostics", "viewer-remote", "tsreadex")
RUST = ("app", *CRATES)
NATIVE = ("connection", "desktop-media", "localization", "subtitle-outline")
GUI = ("danmaku", "ui-style", "channel-wheel", "startup", "screenshot", "video-item",
       "pointer-activity", "portal-dialogs", "subtitle-rendering")
GROUPS = {"rust": RUST, "native": NATIVE, "gui": GUI,
          "cpu": ("checks", *RUST, *NATIVE), "all": ("checks", *RUST, *NATIVE, *GUI)}


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
    if "checks" in suites:
        checks = [Step("format", ["cargo", "fmt", "--manifest-path", "rust/Cargo.toml", "--check"])]
        for name in ("check-release-metadata", "check-ui-style", "flatpak-cargo-sources"):
            checks.append(Step(name, [sys.executable, f"scripts/{name}.py",
                                     *(["--check"] if name == "flatpak-cargo-sources" else [])]))
        checks.extend(Step(path.stem, [sys.executable, str(path)])
                      for path in sorted((ROOT / "scripts").glob("test-*.py")))
        checks.append(Step("comment-sql", [sys.executable, "scripts/check-comment-sql.py"]))
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
    if any(suite in (*NATIVE, *GUI) for suite in suites):
        build.append(Step("build-qt", [sys.executable, "scripts/run-test-binary.py",
                                      "--prepare", str(directory / "nagametv")]))
    if "checks" in suites and "app" in suites:
        tests.insert(0, Step("qml-lint", ["bash", "scripts/check-qml.sh"]))
    tests.extend(Step(suite, ["bash", f"scripts/test-{suite}.sh"])
                 for suite in suites if suite in (*NATIVE, *GUI))
    if "connection" in suites:
        tests.append(Step("cli", [sys.executable, "scripts/check-cli.py", str(directory / "nagametv")]))
    if any(suite in GUI for suite in suites):
        # A missing required resource stops the run before dependent builds/tests.
        checks.insert(0, Step("gui-environment", [sys.executable, "scripts/run-gui-tests.py", "--check"],
                              environment_check=True))
    return [*checks, *build, *tests]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("suites", nargs="*", help="cpu, rust, native, gui, all, or individual suite names")
    parser.add_argument("--list", action="store_true", help="list suites without building or accessing hardware")
    parser.add_argument("--profile", choices=("dev", "release"), default="release")
    parser.add_argument("--log-dir", type=Path, default=ROOT / "build/test-runs",
                        help="parent directory for per-run logs and binaries")
    parser.add_argument("--test-threads", type=int, help="nextest concurrency (default: 2)")
    parser.add_argument("--filter", help="nextest filterset; only allowed with Rust suites")
    args = parser.parse_args()
    if args.list:
        for group, members in GROUPS.items():
            print(f"{group}: {', '.join(members)}")
        return 0
    suites = []
    for name in args.suites or ["cpu"]:
        if name not in (*GROUPS, "checks", *RUST, *NATIVE, *GUI):
            parser.error(f"unknown suite: {name}; use --list")
        suites.extend(suite for suite in GROUPS.get(name, (name,)) if suite not in suites)
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
