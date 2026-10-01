#!/usr/bin/env python3
"""Measure fixed subtitle phases in the validated private GUI session.

Usage: python3 -m scripts.dev.benchmark_subtitle_memory NEW_OUTPUT_DIRECTORY
The harness uses no video stream or synthetic keyboard/pointer input.
"""

import argparse
import json
import os
from pathlib import Path
import selectors
import subprocess
import sys
import time

from scripts.build.support import build_environment, ensure_lock
from scripts.testing.binary import prepare
from scripts.testing.gui_session import require_inherited_session, validate_renderer

def validate_graphics() -> str:
    require_inherited_session(os.environ)
    renderer = subprocess.check_output(["glxinfo", "-B"], text=True)
    validate_renderer(renderer)
    return renderer


def build_helper(output: Path) -> Path:
    binary = output / "subtitle-rendering-test"
    prepare(binary, build_environment("dev"), evaluation=False)
    return binary


def sample_process(pid: int, phase: str, elapsed: float, output: Path) -> dict:
    entry = {"phase": phase, "elapsed_s": round(elapsed, 3)}
    for name in ("status", "smaps_rollup"):
        data = Path(f"/proc/{pid}/{name}").read_text()
        (output / f"{phase}-{name}.txt").write_text(data)
        for line in data.splitlines():
            key, _, value = line.partition(":")
            if key in ("VmRSS", "Rss", "Anonymous", "Private_Dirty", "Pss", "Threads"):
                entry[key] = int(value.split()[0])
    return entry


def measure(repo: Path, output: Path, binary: Path, stroke: bool) -> None:
    env = os.environ.copy()
    env.update(QT_QPA_PLATFORM="xcb", QSG_RHI_BACKEND="opengl", MALLOC_MMAP_THRESHOLD_="131072")
    env["VIEWER_SUBTITLE_BENCHMARK_STROKE"] = "1" if stroke else "0"
    (output / "conditions.json").write_text(json.dumps({
        "display": env["DISPLAY"], "stroke": stroke,
        "test_runner": "Rust native_tests (Cargo dev profile)",
        "mmap_threshold_bytes": 131072, "renderer": "opengl",
    }, indent=2) + "\n")
    # An inherited glibc tunable takes precedence over the legacy environment
    # variable. Preserve other tunables, but make this benchmark's threshold explicit.
    tunables = [value for value in env.get("GLIBC_TUNABLES", "").split(":")
                if value and not value.startswith("glibc.malloc.mmap_threshold=")]
    env["GLIBC_TUNABLES"] = ":".join(tunables + ["glibc.malloc.mmap_threshold=131072"])
    expected = ["loaded"] + [f"{phase}{round_number}" for round_number in range(1, 6)
                             for phase in ("round", "clear")]
    command = [str(binary), "--native-tests", "subtitle-rendering", "-input", str(repo / "tests/subtitle-memory/tst_SubtitleMemory.qml"),
               "-o", "-,txt"]
    process = subprocess.Popen(command, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    (output / "pid").write_text(str(process.pid))
    print(f"GPU subtitle benchmark pid={process.pid}", flush=True)
    samples, log, pending = [], bytearray(), bytearray()
    started = time.monotonic()
    selector = selectors.DefaultSelector()
    selector.register(process.stdout, selectors.EVENT_READ)
    try:
        while selector.get_map():
            if time.monotonic() - started > 60:
                raise TimeoutError("60-second rendering deadline exceeded")
            for key, _ in selector.select(timeout=1):
                chunk = os.read(key.fileobj.fileno(), 65536)
                if not chunk:
                    selector.unregister(key.fileobj)
                    continue
                log.extend(chunk)
                pending.extend(chunk)
                if len(log) > 1024 * 1024:
                    raise RuntimeError("1MiB log limit exceeded")
                while b"\n" in pending:
                    line, _, pending = pending.partition(b"\n")
                    if b"MEMORY_PHASE " not in line:
                        continue
                    phase = line.split(b"MEMORY_PHASE ", 1)[1].decode().strip()
                    if len(samples) >= len(expected) or phase != expected[len(samples)]:
                        raise RuntimeError(f"Unexpected measurement phase: {phase}")
                    sample = sample_process(process.pid, phase, time.monotonic() - started, output)
                    samples.append(sample)
                    print(json.dumps(sample), flush=True)
        code = process.wait(timeout=5)
        if code:
            raise RuntimeError(f"Rendering test exited {code}")
        if len(samples) != len(expected):
            raise RuntimeError("Incomplete phase measurements")
    finally:
        if process.poll() is None:
            process.kill()
            process.wait()
        selector.close()
        process.stdout.close()
        (output / "player.log").write_bytes(log)
        (output / "samples.json").write_text(json.dumps(samples, indent=2) + "\n")
    print("GPU benchmark exit=0", flush=True)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument("--no-outline", action="store_true", help="Disable stroke in the test corpus only")
    args = parser.parse_args()
    if "NAGAMETV_GUI_SESSION_DIR" not in os.environ:
        return subprocess.call([sys.executable, "-m", "scripts.testing.gui_session", "--",
                                sys.executable, "-m", "scripts.dev.benchmark_subtitle_memory", *sys.argv[1:]])
    ensure_lock()
    renderer = validate_graphics()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    (output / "renderer.txt").write_text(renderer)
    repo = Path(__file__).resolve().parents[2]
    # HEAD alone does not describe an uncommitted rendering experiment. Preserve
    # the actual components and harness alongside its measurements.
    for relative in ("rust/qml/SubtitleGlyph.qml", "rust/qml/SubtitleOverlay.qml",
                     "rust/src/subtitle_outline.h", "rust/src/native_test_bridge.rs",
                     "rust/src/native_tests/mod.rs", "rust/src/native_tests/qt_test_api.h",
                     "rust/src/native_tests/localization.rs", "rust/src/native_tests/outline.rs",
                     "rust/src/native_tests/pointer.rs", "tests/localization.qml",
                     "rust/src/main.rs", "rust/src/danmaku_ui_tests.rs",
                     "rust/src/danmaku_test.h", "rust/Cargo.toml", "rust/Cargo.lock", "rust/build.rs",
                     "tests/subtitle-memory/tst_SubtitleMemory.qml",
                     "scripts/dev/benchmark_subtitle_memory.py"):
        destination = output / "sources" / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes((repo / relative).read_bytes())
    with (output / "source-head.txt").open("w") as destination:
        subprocess.run(["git", "-C", str(repo), "rev-parse", "HEAD"], stdout=destination, check=True)
    binary = build_helper(output)
    measure(repo, output, binary, stroke=not args.no_outline)


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, RuntimeError, subprocess.SubprocessError) as error:
        print(f"Subtitle memory benchmark failed: {error}", file=sys.stderr)
        sys.exit(1)
