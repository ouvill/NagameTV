#!/usr/bin/env python3
"""Regenerate the AV1 parser regression fixture using CPU encoding only.

Requires gst-launch-1.0, videotestsrc, av1enc (libaom) and mp4mux.
No decoder, display, GPU or audio device is used.
"""
from pathlib import Path
import subprocess


def main():
    output = Path(__file__).resolve().with_name("av1-timestamps.mp4")
    subprocess.run([
        "gst-launch-1.0", "-q", "videotestsrc", "num-buffers=32", "pattern=ball", "!",
        "video/x-raw,format=I420,width=64,height=64,framerate=30/1", "!",
        "av1enc", "cpu-used=6", "threads=1", "lag-in-frames=16", "!",
        "mp4mux", "!", "filesink", f"location={output}",
    ], check=True, timeout=60)
    print(f"{output.name}: {output.stat().st_size} bytes")


if __name__ == "__main__":
    main()
