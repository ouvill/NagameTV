# Subtitle clock fixture

`subtitle-clock.ts` is a generated two-second, 64×64 black MPEG-2 video (25 fps),
with no audio or broadcast content. The subtitle clock regression test demuxes
it to memory and compares transport PTS to GStreamer video stream time.

Generate with GStreamer (CPU only, no display or audio devices):

```sh
gst-launch-1.0 -q videotestsrc num-buffers=50 pattern=black \
  ! video/x-raw,width=64,height=64,framerate=25/1 \
  ! avenc_mpeg2video ! mpegvideoparse ! mpegtsmux \
  ! filesink location=tests/fixtures/subtitle-clock.ts
```

## Recording playback fixture

`recording.ts` contains three seconds of synthetic 160×90 MPEG-2 video (25 fps)
and silent stereo AAC. It exercises local file playback in the production UI.
It contains no broadcast material. Generate using CPU-only sources and encoders:

```sh
gst-launch-1.0 -q mpegtsmux name=mux ! filesink location=tests/fixtures/recording.ts \
  videotestsrc num-buffers=75 pattern=ball \
    ! video/x-raw,width=160,height=90,framerate=25/1 \
    ! avenc_mpeg2video ! mpegvideoparse ! queue ! mux. \
  audiotestsrc num-buffers=141 wave=silence \
    ! audio/x-raw,rate=48000,channels=2 ! audioconvert ! avenc_aac ! aacparse ! queue ! mux.
```

## Seek and TS program information fixture

`recording-seek.ts` is 60 seconds of synthetic MPEG-2 video and silent AAC.
Service/TS/network ID 1 carries two 30-second programs, Japanese titles and a
station name, an extended description, a content descriptor and JST TOT time.
SI repeats once per second. Generate without display, GPU or audio devices:

```sh
gst-launch-1.0 -q mpegtsmux name=mux ! filesink location=tests/fixtures/recording-seek.ts \
  videotestsrc num-buffers=1500 pattern=ball \
    ! video/x-raw,width=160,height=90,framerate=25/1 \
    ! avenc_mpeg2video ! mpegvideoparse ! queue ! mux. \
  audiotestsrc num-buffers=2813 wave=silence \
    ! audio/x-raw,rate=48000,channels=2 ! audioconvert ! avenc_aac ! aacparse ! queue ! mux.
python3 scripts/fixtures/recording-si.py
```

CPU tests exercise demux/decode into an in-memory sink. The native startup test
uses the same file with the production GL/video/audio graph and Main.qml.
Additional PAT/PMT, caption and malformed SI cases are synthesized by Rust tests.

## Recording recovery fixtures

Regenerate both files deterministically from the checked-in `recording.ts`, using
only the Python standard library and no hardware:

```sh
python3 scripts/fixtures/recording-recovery.py
```

Each file joins two copies of the three-second recording. The second half begins
with adaptation-only discontinuity packets; both halves keep service ID 1.

| File | Change at the boundary |
| --- | --- |
| `recording-pid-change.ts` | PAT/PMT version 0 → 1; PMT PID `0x20` → `0x120`, video/PCR `0x41` → `0x141`, audio `0x42` → `0x142`. PCR/PTS/DTS advance by 3.2 seconds, including a 200 ms margin. |
| `recording-clock-reset.ts` | Same PAT/PMT and PIDs; PCR/PTS/DTS restart at the original values. |

The hardware-free `transport::recovery_tests` validates packet parsing, table
CRC/version/service, video PES counts and PCR changes. The startup suite plays
the clock-reset file through the production window and requires frames from both
halves and normal EOF. PID changes currently fail in production playback; run
`bash scripts/test-startup.sh recording-pid-change` as a separate failing
reproducer. Both UI commands validate display, GPU and audio access first.

Subtitle recovery uses authored ARIB management, statement and DRCS data groups
in `rust/src/features/subtitles/stream_selection_tests.rs`, without broadcast
content. These tests cover decoder state and queued captions; arbitrary DRCS
bitmap rendering is outside the existing UI adapter's capabilities.
