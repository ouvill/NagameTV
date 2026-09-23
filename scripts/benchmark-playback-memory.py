#!/usr/bin/env python3
"""Linux process measurements in the validated private GUI session (no host desktop)."""

import argparse
from contextlib import contextmanager
import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import shutil
import statistics
import subprocess
import sys
import threading
import time

ROOT = Path(__file__).resolve().parents[1]
PACKET_BYTES = 188
TS_SYNC_BYTE = 0x47
ADAPTATION_FLAG = 0x20
ADAPTATION_CONTROL_OFFSET = 3
ADAPTATION_LENGTH_OFFSET = 4
ADAPTATION_FLAGS_OFFSET = 5
PCR_FLAG = 0x10
PCR_OFFSET = 6
PCR_BYTES = 6
MIN_PCR_ADAPTATION_BYTES = 7
PCR_HZ = 90000
PCR_WRAP = 1 << 33
SAMPLE_SECONDS = 1
STOP_SECONDS = 10
WARMUP_SECONDS = 10
MIN_MEASUREMENT_SECONDS = 25
MAX_PCR_GAP_BYTES = 4 * 1024 * 1024
# /proc/PID/stat indexes after removing pid and the parenthesized command.
STAT_USER_TICKS_INDEX = 11
STAT_SYSTEM_TICKS_INDEX = 12


def pcr(packet):
    if len(packet) != PACKET_BYTES or packet[0] != TS_SYNC_BYTE:
        raise ValueError("Input must contain aligned 188-byte TS packets")
    if (packet[ADAPTATION_CONTROL_OFFSET] & ADAPTATION_FLAG
            and packet[ADAPTATION_LENGTH_OFFSET] >= MIN_PCR_ADAPTATION_BYTES
            and packet[ADAPTATION_FLAGS_OFFSET] & PCR_FLAG):
        p = packet[PCR_OFFSET:PCR_OFFSET + PCR_BYTES]
        return (p[0] << 25) | (p[1] << 17) | (p[2] << 9) | (p[3] << 1) | (p[4] >> 7)
    return None


class Server(ThreadingHTTPServer):
    daemon_threads = True

    def __init__(self, source, service):
        self.source = source
        self.service = service
        self.stopping = threading.Event()
        self.errors = []
        super().__init__(("127.0.0.1", 0), Handler)


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def do_GET(self):
        try:
            if self.path == "/api/services":
                self.respond([dict(id=1, serviceId=self.server.service, networkId=1,
                                   name="Memory benchmark", type=1,
                                   channel=dict(type="GR", channel="1"))])
            elif self.path == "/api/programs":
                self.respond([])
            elif self.path.startswith("/api/events/stream"):
                self.send_response(200)
                self.end_headers()
                self.wfile.write(b"[")
                self.wfile.flush()
                self.server.stopping.wait()
            elif self.path == "/api/services/1/stream":
                self.send_response(200)
                self.send_header("Content-Type", "video/MP2T")
                self.end_headers()
                first = None
                started = time.monotonic()
                buffered = bytearray()
                with self.server.source.open("rb") as source:
                    while packet := source.read(PACKET_BYTES):
                        if self.server.stopping.is_set():
                            return
                        timestamp = pcr(packet)
                        buffered.extend(packet)
                        if len(buffered) > MAX_PCR_GAP_BYTES:
                            raise ValueError("No PCR within 4 MiB of TS")
                        if timestamp is None:
                            continue
                        if first is None:
                            first = timestamp
                        elapsed = ((timestamp - first) % PCR_WRAP) / PCR_HZ
                        if self.server.stopping.wait(max(0, started + elapsed - time.monotonic())):
                            return
                        self.wfile.write(buffered)
                        buffered.clear()
                raise ValueError("TS ended before measurement completed; supply a longer fixture")
            else:
                self.send_error(404)
        except (BrokenPipeError, ConnectionResetError):
            pass
        except Exception as error:
            self.server.errors.append(str(error))

    def respond(self, value):
        body = json.dumps(value).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)


@contextmanager
def fixture(source, service):
    with Server(source, service) as server:
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            yield server
        finally:
            server.stopping.set()
            server.shutdown()
            thread.join()


def sample(pid, elapsed):
    proc = Path("/proc") / str(pid)
    values = {"seconds": elapsed}
    for filename in ("status", "smaps_rollup"):
        for line in (proc / filename).read_text().splitlines():
            key, _, rest = line.partition(":")
            if key in ("Rss", "Pss", "Private_Clean", "Private_Dirty", "Anonymous", "Threads"):
                values[key] = int(rest.split()[0])
    values["Uss"] = values["Private_Clean"] + values["Private_Dirty"]
    # /proc/PID/stat's command field may contain spaces or parentheses.
    fields = (proc / "stat").read_text().rsplit(")", 1)[1].split()
    values["cpu_seconds"] = (int(fields[STAT_USER_TICKS_INDEX])
                             + int(fields[STAT_SYSTEM_TICKS_INDEX])) / os.sysconf("SC_CLK_TCK")
    values["fds"] = len(list((proc / "fd").iterdir()))
    return values


def run(binary, phase, output, args):
    output.mkdir(parents=True)
    env = {key: value for key, value in os.environ.items()
           if not key.startswith("NAGAMETV_") or key.startswith("NAGAMETV_GUI_")}
    for key, folder in (("XDG_CONFIG_HOME", "config"), ("XDG_CACHE_HOME", "cache"),
                        ("XDG_STATE_HOME", "state"), ("XDG_DATA_HOME", "data")):
        env[key] = str(output / folder)
    env.update(NAGAMETV_AUTOPLAY="1" if phase == "playback" else "0",
               NAGAMETV_DIAGNOSTICS="1", NAGAMETV_GC_LOG="0",
               NAGAMETV_AUDIO_SINK="pulsesink", NAGAMETV_REMOTE_ENABLED="0",
               NAGAMETV_DEINTERLACE=args.deinterlace)
    with fixture(args.ts, args.service_id) as server, (output / "app.log").open("w") as log:
        env["NAGAMETV_SERVER"] = f"http://127.0.0.1:{server.server_port}"
        env["NAGAMETV_SERVICE_ID"] = "1"
        process = subprocess.Popen([str(binary)], env=env, stdout=log, stderr=log)
        started = time.monotonic()
        samples = []
        window = None
        window_seconds = None
        try:
            for second in range(SAMPLE_SECONDS, args.seconds + 1, SAMPLE_SECONDS):
                time.sleep(max(0, started + second - time.monotonic()))
                if process.poll() is not None or server.errors:
                    raise RuntimeError(f"Application/fixture failed: {server.errors}; see {output}")
                samples.append(sample(process.pid, time.monotonic() - started))
                if window is None:
                    found = subprocess.run(["xdotool", "search", "--onlyvisible", "--pid", str(process.pid)],
                                           capture_output=True, text=True, timeout=STOP_SECONDS)
                    if found.returncode == 0:
                        window = found.stdout.splitlines()[0]
                        window_seconds = time.monotonic() - started
            if window is None:
                raise RuntimeError("Application did not display a window")
            (output / "smaps.txt").write_text((Path("/proc") / str(process.pid) / "smaps").read_text())
            subprocess.run(["xdotool", "windowactivate", "--sync", window], check=True, timeout=STOP_SECONDS)
            subprocess.run(["xdotool", "key", "--clearmodifiers", "alt+F4"], check=True, timeout=STOP_SECONDS)
            process.wait(timeout=STOP_SECONDS)
            if process.returncode:
                raise RuntimeError(f"Application shutdown failed: {process.returncode}")
        finally:
            (output / "samples.json").write_text(json.dumps(samples, indent=2) + "\n")
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=STOP_SECONDS)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()
    snapshots = []
    for path in (output / "state/nagametv/usage").glob("usage-*.jsonl"):
        for line in path.read_text().splitlines():
            record = json.loads(line).get("record", {})
            if "snapshot" in record:
                snapshots.append(dict(record["snapshot"], elapsed_ms=record["elapsed_ms"]))
    rendered = [s["video_rendered"] for s in snapshots if s.get("video_rendered") is not None]
    if phase == "playback" and (not rendered or max(rendered) == 0):
        raise RuntimeError(f"No rendered frames recorded; see {output}")
    steady = [s for s in samples if s["seconds"] >= WARMUP_SECONDS]
    result = {key: statistics.median(s[key] for s in steady)
              for key in ("Rss", "Pss", "Uss", "Anonymous", "Threads", "fds")}
    result["cpu_percent"] = 100 * (steady[-1]["cpu_seconds"] - steady[0]["cpu_seconds"]) / (
        steady[-1]["seconds"] - steady[0]["seconds"])
    result["sampled_peak_rss_kib"] = max(s["Rss"] for s in samples)
    result["window_observed_seconds"] = window_seconds
    result["video_rendered"] = max(rendered, default=None)
    result["video_dropped"] = max((s.get("video_dropped") or 0 for s in snapshots), default=0)
    if phase == "playback":
        frames = [s for s in snapshots if s["elapsed_ms"] >= WARMUP_SECONDS * 1000
                  and s.get("video_rendered") is not None]
        if len(frames) < 2 or frames[-1]["video_rendered"] <= frames[0]["video_rendered"]:
            raise RuntimeError(f"Playback did not continue through measurement; see {output}")
        result["steady_rendered_fps"] = (
            (frames[-1]["video_rendered"] - frames[0]["video_rendered"]) * 1000
            / (frames[-1]["elapsed_ms"] - frames[0]["elapsed_ms"]))
        result["steady_dropped"] = frames[-1]["video_dropped"] - frames[0]["video_dropped"]
    (output / "summary.json").write_text(json.dumps(result, indent=2) + "\n")
    print(output.name, json.dumps(result), flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path, help="New output directory (measurements in KiB)")
    parser.add_argument("--binary", type=Path, action="append", required=True,
                        help="Repeat for interleaved before/after comparisons")
    parser.add_argument("--ts", type=Path, required=True, help="188-byte TS with continuous PCR, longer than the run")
    parser.add_argument("--service-id", type=int, default=1)
    parser.add_argument("--seconds", type=int, default=35)
    parser.add_argument("--repeat", type=int, default=3)
    parser.add_argument("--phase", choices=("idle", "playback"), action="append")
    parser.add_argument("--deinterlace", choices=("yadif", "linear", "off", "gl"), default="yadif")
    args = parser.parse_args()
    if args.seconds < MIN_MEASUREMENT_SECONDS or args.repeat < 1 or not 1 <= args.service_id <= 65535:
        parser.error("seconds must be at least 25, repeat must be positive, service-id must be 1..65535")
    args.ts = args.ts.resolve(strict=True)
    args.binary = [binary.resolve(strict=True) for binary in args.binary]
    args.output = args.output.resolve()
    # The public launcher detects and validates GPU, private display and audio.
    if "NAGAMETV_GUI_SESSION_DIR" not in os.environ:
        return subprocess.call([sys.executable, str(ROOT / "scripts/run-gui-tests.py"), "--",
                                sys.executable, str(Path(__file__).resolve()), *sys.argv[1:]])
    session = Path(os.environ["NAGAMETV_GUI_SESSION_DIR"])
    if os.environ.get("DISPLAY") != (session / "display").read_text().strip():
        raise RuntimeError("Use the validated private GUI display")
    if shutil.which("xdotool") is None:
        raise RuntimeError("xdotool is required to close the product window")
    args.output.mkdir(parents=True, exist_ok=False)
    metadata = dict(arguments=sys.argv[1:], units="KiB", cpu_percent="100 = one logical CPU",
                    session=str(session), binaries=[str(b) for b in args.binary],
                    build_info=[json.loads(subprocess.check_output([str(b), "--build-info"])) for b in args.binary])
    with args.ts.open("rb") as source:
        metadata["ts_sha256"] = hashlib.file_digest(source, "sha256").hexdigest()
    metadata["binary_sha256"] = []
    for binary in args.binary:
        with binary.open("rb") as source:
            metadata["binary_sha256"].append(hashlib.file_digest(source, "sha256").hexdigest())
    metadata["environment"] = {key: os.environ[key] for key in (
        "QT_QPA_PLATFORM", "QSG_RHI_BACKEND", "GST_PLUGIN_FEATURE_RANK",
        "MALLOC_ARENA_MAX", "GLIBC_TUNABLES", "LD_PRELOAD") if key in os.environ}
    (args.output / "metadata.json").write_text(json.dumps(metadata, indent=2) + "\n")
    for repeat in range(1, args.repeat + 1):
        for phase in args.phase or ("idle", "playback"):
            for index, binary in enumerate(args.binary):
                run(binary, phase, args.output / f"binary-{index + 1}-{phase}-{repeat}", args)
    return 0


if __name__ == "__main__":
    sys.exit(main())
