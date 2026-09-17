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
CRC/version/service, video PES counts and PCR changes. These checks are not a
complete MPEG-TS conformance certification. The startup suite plays both files
through the production window and requires frames from both halves and normal
EOF. Sink rendering counters reset at the PID transition; the test sums observed
increments across resets. The earlier PID playback failure was a test assertion
error, corrected on 2026-09-17. Run
`bash scripts/test-startup.sh recording-pid-change` for the targeted regression
check. Both UI commands validate display, GPU and audio access first.

`scripts/fixtures/recording-output-audit.py` counts decoded buffers per stream ID
with CPU MPEG-2/AAC decoders and explicit memory sinks. It requires PyGObject,
GStreamer introspection and libav, and never uses display, GPU or audio devices.
`--paced` synchronizes to the clock; the deadline is 12 seconds, intended for the
short recovery fixtures. The [tsreadex trial](../../docs/tsreadex-trial.md) uses it
alongside the hardware-validated production `recording-audit` suite.

Subtitle recovery uses authored ARIB management, statement and DRCS data groups
in `rust/src/features/subtitles/stream_selection_tests.rs`, without broadcast
content. These tests cover decoder state and queued captions; arbitrary DRCS
bitmap rendering is outside the existing UI adapter's capabilities.

## Caption stream addition near 15 seconds

`recording-caption-change.ts` is a 60-second synthetic recording with two closely
spaced PMT changes. It is generated from `recording-seek.ts`; all original video,
primary audio, PCR/PTS/DTS, PAT and SI packets remain byte-identical. The added
second audio copies the synthetic silent AAC, starting at a complete PES header.
All caption text and management packets are authored, without broadcast payload.

| Elapsed PCR time | PMT version | Stream configuration |
| --- | --- | --- |
| Start | 0 | MPEG-2 video, one AAC audio stream, superimpose management; no caption stream |
| 15.04 seconds | 1 | Add ARIB caption PID `0x130`, component tag `0x30` |
| 15.20 seconds | 2 | Keep the captions and add second AAC audio PID `0x43` |

The generator schedules the changes at 15.0 and 15.2 seconds, emitting them after
the next video PCR (80 ms intervals in the input fixture). Superimpose PID
`0x138`/tag `0x38` remains present throughout. Captions display `CAPTION 15s`
through `CAPTION 59s`, once per second, in a 960×540 plane. Normal-size ARIB
alphanumeric characters decode to fullwidth Unicode.

This models the structure observed in the user-provided 2026-09-15 News Watch 9
recording: caption PID `0x130` appeared around 14.806 seconds (PMT version
11 → 12), followed by audio PID `0x111` around 14.979 seconds (version 12 → 13).
Those observed times are relative to the first PCR, not an exact UI timestamp.
The synthetic file deliberately omits broadcast data carousels, CA descriptors,
and the original pictures, audio and caption text. It reproduces the stream
additions, not the entire original multiplex or every possible decoder failure.

Regenerate deterministically using Python's standard library, without hardware:

```sh
python3 scripts/fixtures/recording-caption-change.py
# Isolate caption addition from the second audio/PMT change.
mkdir -p benchmark/caption-transition
python3 scripts/fixtures/recording-caption-change.py --scenario caption-only \
  --output benchmark/caption-transition/caption-only.ts
```

The Rust regression tests validate all PMT CRCs and versions, transport continuity,
announced stream PIDs and transition times, exact preservation of the base media,
and all 45 captions through the real parser/libaribcaption. They test both raw
input and the production tsreadex filter's output, including the first caption
before the second audio addition. Run them without hardware:

```sh
CARGO_TARGET_DIR=build/cargo cargo test --manifest-path rust/Cargo.toml \
  --release --locked caption_transition
```

Compare native playbin3 decoding with the existing CPU-only audit. These commands
use explicit memory sinks and a CPU decoder allowlist; do not add `--paced` to
this 60-second fixture because that audit has a 12-second wall-clock deadline.

```sh
python3 scripts/fixtures/recording-output-audit.py tests/fixtures/recording-caption-change.ts
python3 scripts/fixtures/recording-output-audit.py benchmark/caption-transition/caption-only.ts
cargo run --quiet --manifest-path rust/crates/tsreadex/Cargo.toml --locked \
  --example filter < tests/fixtures/recording-caption-change.ts \
  > benchmark/caption-transition/normalized.ts
python3 scripts/fixtures/recording-output-audit.py benchmark/caption-transition/normalized.ts
```

On GStreamer 1.28.2 (2026-09-17), the raw two-change case failed near 15 seconds
with `No valid frames found before end of stream`. The caption-only case and
normalized two-change case reached EOF. PMT changes replaced video/audio demux
pads even though the existing media PIDs did not change. Normalization removed
the caption-addition transition, leaving the second-audio transition. EOF did
not imply lossless playback: the CPU audit still observed a small video frame
loss around the remaining transition. Raw failure counters vary with streaming
thread timing; they are diagnostic observations, not stable test assertions.
These results reproduce a related failure condition, without proving the cause
of the original recording's stall or verifying production GUI playback.
Local audit logs are under the ignored `benchmark/caption-transition/` directory.

### Player comparison and parser lifetime

The same unnormalized fixture reached EOF with mpv 0.41.0 / FFmpeg 8.0.1 in a
CPU-only decode comparison. Video and audio were explicitly discarded; this
checks decoding through the transition, not displayed frames or audible output.
VLC playback was reported by the user and was not rerun in the container.

```sh
mpv --no-config --ytdl=no --hwdec=no --vo=null --ao=null --untimed \
  --audio-display=no --log-file=benchmark/caption-transition/mpv-raw.log \
  tests/fixtures/recording-caption-change.ts
```

GStreamer debug logging narrows the synthetic failure to the intermediate video
parser's lifetime. Adding caption PID `0x130` creates `video_1_0041` and
`mpegvparse1`. That parser reports missing configuration while dropping frames.
Adding audio PID `0x43` creates `video_2_0041` and ends the previous stream;
`mpegvparse1` then reports no valid frames before EOS. The sequence headers
surrounding the transition occur at relative video PTS 14.88 and 15.36 seconds.
The existing video payload was not changed by the fixture generator.

This agrees with the [GStreamer 1.28.2 PMT update implementation](https://github.com/GStreamer/gstreamer/blob/1.28.2/subprojects/gst-plugins-bad/gst/mpegtsdemux/mpegtsbase.c),
where incremental program updates are disabled and an active changed program is
replaced. These observations identify a failure in this GStreamer playback path;
successful playback in mpv is compatible with the fixture's purpose. The fixture
must not be described as a universally unplayable or proven-corrupt TS.
The original recording's stall still requires separate cause confirmation.

The current decision is to retain the production tsreadex normalization. Revisit
this workaround when upstream handles these transitions successfully, using this
fixture to compare behavior. An alternative FFmpeg-based input path is another
option, but it would need to cover TS demuxing and stream reconfiguration: the
failing CPU audit already uses the libav video/audio decoders, so replacing only
the decoder is not an established fix. No playback backend change is included.

Validation: 216 Rust tests passed, 3 intentionally ignored; release Clippy for all
targets passed with warnings denied. Generation is deterministic and requires no
original broadcast recording.
