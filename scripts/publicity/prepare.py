#!/usr/bin/env python3
"""Download a pinned CC BY movie and transcode a short fixture on the CPU."""
import hashlib
from pathlib import Path
import shutil
import subprocess
import urllib.request
import zipfile

ROOT = Path(__file__).resolve().parents[2]
BASE = ROOT / "build/publicity"
ARCHIVE_NAME = "big_buck_bunny_720p_stereo.ogg.zip"
MOVIE_NAME = "big_buck_bunny_720p_stereo.ogg"
SOURCE_URL = "https://download.blender.org/peach/bigbuckbunny_movies/" + ARCHIVE_NAME
ARCHIVE_SHA256 = "dc24016805203e5ae750f19cbd616e20561664d5bf5755ae484e3ed5487824b2"
DOWNLOAD_TIMEOUT_SECONDS = 60
ENCODE_TIMEOUT_SECONDS = 180
CLIP_START_SECONDS = 78
CLIP_DURATION_SECONDS = 120


def main():
    if shutil.which("ffmpeg") is None:
        raise SystemExit("FFmpeg is required; see docs/publicity.md")
    source_dir = BASE / "source"
    source_dir.mkdir(parents=True, exist_ok=True)
    archive = source_dir / ARCHIVE_NAME
    if not archive.exists():
        print("Downloading Big Buck Bunny (188 MiB). Credits: docs/media/README.md", flush=True)
        temporary = archive.with_suffix(".download")
        try:
            with urllib.request.urlopen(SOURCE_URL, timeout=DOWNLOAD_TIMEOUT_SECONDS) as response:
                with temporary.open("wb") as output:
                    shutil.copyfileobj(response, output)
            temporary.replace(archive)
        finally:
            temporary.unlink(missing_ok=True)
    with archive.open("rb") as source:
        if hashlib.file_digest(source, "sha256").hexdigest() != ARCHIVE_SHA256:
            raise SystemExit("Movie archive checksum mismatch; inspect the source before proceeding.")
    movie = source_dir / "big-buck-bunny.ogg"
    with zipfile.ZipFile(archive) as source:
        with source.open(MOVIE_NAME) as member, movie.open("wb") as output:
            shutil.copyfileobj(member, output)
    temporary = BASE / "demo.part.ts"
    try:
        # Explicit CPU codecs; this preparation never opens display/audio/GPU devices.
        subprocess.run([
            "ffmpeg", "-nostdin", "-hide_banner", "-loglevel", "error",
            "-ss", str(CLIP_START_SECONDS), "-i", str(movie), "-t", str(CLIP_DURATION_SECONDS),
            "-map", "0:v:0", "-map", "0:a:0", "-c:v", "mpeg2video", "-b:v", "6M",
            "-maxrate", "8M", "-bufsize", "16M", "-r", "30", "-bf", "0", "-g", "15",
            "-c:a", "aac", "-b:a", "128k", "-ar", "48000", "-ac", "2",
            "-mpegts_service_id", "1", "-y", str(temporary),
        ], check=True, timeout=ENCODE_TIMEOUT_SECONDS)
        temporary.replace(BASE / "demo.ts")
    finally:
        temporary.unlink(missing_ok=True)
    print("Prepared build/publicity/demo.ts (120 seconds, 1280×720, 30 fps).")


if __name__ == "__main__":
    main()
