#!/usr/bin/env python3
"""Generate 12-second MP4/Matroska fixtures with CPU encoders only.

Synthetic video + audible test tone; files are never played by this script.
No display, GPU or audio device is used. Requires GStreamer openh264, x265,
libav, isomp4 and matroska plugins.
"""
from pathlib import Path
import subprocess

root = Path(__file__).resolve().parents[2]
for codec, pixel_format in [('h264', 'I420'), ('hevc', 'I420'), ('hevc-10bit', 'I420_10LE')]:
    encoder = (
        ['openh264enc', 'gop-size=25', 'bitrate=150000', '!', 'h264parse']
        if codec == 'h264' else
        ['x265enc', 'key-int-max=25', 'bitrate=150', 'speed-preset=ultrafast',
         'option-string=pools=none:frame-threads=1:log-level=error', '!', 'h265parse']
    )
    for container, mux in [('mp4', 'mp4mux'), ('mkv', 'matroskamux')]:
        if codec == 'hevc-10bit' and container != 'mp4':
            continue
        output = root / 'tests' / 'fixtures' / f'media-{codec}.{container}'
        # Cover both tail and front MP4 indexes without duplicate media fixtures.
        mux_options = ['faststart=true'] if container == 'mp4' and codec.startswith('hevc') else []
        subprocess.run([
            'gst-launch-1.0', '-q', mux, 'name=mux', *mux_options, '!', 'filesink', f'location={output}',
            'videotestsrc', 'num-buffers=300', 'pattern=ball', '!',
            f'video/x-raw,format={pixel_format},width=160,height=96,framerate=25/1', '!',
            *encoder, '!', 'queue', '!', 'mux.',
            'audiotestsrc', 'num-buffers=563', 'samplesperbuffer=1024', 'volume=0.05', '!',
            'audio/x-raw,rate=48000,channels=2', '!', 'audioconvert', '!',
            'avenc_aac', '!', 'aacparse', '!', 'queue', '!', 'mux.',
        ], check=True, timeout=60)
        print(output.relative_to(root), output.stat().st_size, flush=True)
