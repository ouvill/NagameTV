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
python3 tests/fixtures/generate_recording_si.py
```

CPU tests exercise demux/decode into an in-memory sink. The native startup test
uses the same file with the production GL/video/audio graph and Main.qml.
Additional PAT/PMT, caption and malformed SI cases are synthesized by Rust tests.

## Recording recovery fixtures

Regenerate both files deterministically from the checked-in `recording.ts`, using
only the Python standard library and no hardware:

```sh
python3 tests/fixtures/generate_recording_recovery.py
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
`python3 scripts/test.py startup -- recording-pid-change` for the targeted regression
check. Both UI commands validate display, GPU and audio access first.

`scripts/testing/recording_output_audit.py` counts decoded buffers per stream ID
with CPU MPEG-2/AAC decoders and explicit memory sinks. It requires PyGObject,
GStreamer introspection and libav, and never uses display, GPU or audio devices.
`--paced` synchronizes to the clock; the deadline is 12 seconds, intended for the
short recovery fixtures. The [tsreadex trial](../../docs/tsreadex-trial.md) uses it
alongside the hardware-validated production `recording-audit` suite.

Subtitle recovery uses authored ARIB management, statement and DRCS data groups
in `rust/src/features/subtitles/stream_selection_tests.rs`, without broadcast
content. The shared authored packets in
`rust/crates/libaribcaption/tests/fixtures/drcs.rs` also exercise owned bitmap
export and redefinition. Rust image/screenshot tests and `subtitle-rendering`
cover bitmap presentation, outline switching, scaling and capture lifetime.

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
python3 tests/fixtures/generate_recording_caption_change.py
# Isolate caption addition from the second audio/PMT change.
mkdir -p benchmark/caption-transition
python3 tests/fixtures/generate_recording_caption_change.py --scenario caption-only \
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
python3 -m scripts.testing.recording_output_audit tests/fixtures/recording-caption-change.ts
python3 -m scripts.testing.recording_output_audit benchmark/caption-transition/caption-only.ts
cargo run --quiet --manifest-path rust/crates/tsreadex/Cargo.toml --locked \
  --example filter < tests/fixtures/recording-caption-change.ts \
  > benchmark/caption-transition/normalized.ts
python3 -m scripts.testing.recording_output_audit benchmark/caption-transition/normalized.ts
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

## General media fixtures

`media-h264.mp4`, `media-h264.mkv`, `media-hevc.mp4` and `media-hevc.mkv`
contain 12 seconds of synthetic 160×96, 25 fps, 8-bit SDR video and stereo AAC
(48 kHz test tone). They contain no broadcast material. Regenerate with
`python3 tests/fixtures/generate_general_media.py`; this uses CPU OpenH264/x265 and
libav AAC encoders with file outputs, without display, GPU or audio devices.
H.264 MP4 keeps its `moov` index at the end; HEVC MP4 uses faststart.
`media-hevc-10bit.mp4` uses the same pattern and duration in 10-bit SDR. The
startup suite exercises GPU P010 decoding and negotiated display conversion,
including pause, seek, replay and switching back to 8-bit media. Automatic
output must allow RGBA because glcolorconvert cannot convert P010 GL textures
directly to NV12.

The hardware-free `playback::media` tests use explicit CPU decoders and memory
outputs to check both local and HTTP Range input, both tracks, paused forward
and backward seeks, playback rate, content detection independent of extensions,
and replay validation. A CPU-only `playbin3` test also covers automatic
typefinding, the source-setup callback and paused seeking for all four files.
The startup suite additionally uses the real product
window and validated GPU/virtual audio output for source switching, EOF,
EPGStation file selection and token-authenticated playback.

`media-subtitles.mp4` / `media-subtitles.mkv` は `media-h264.mp4` に合成字幕を追加した
試験用動画です。MP4はテキスト字幕1トラック、MKVはテキスト字幕と装飾付きASSの2トラックを
持ちます。外部字幕は `media-subtitles.srt` / `.ass` です。
`/usr/bin/python3 tests/fixtures/generate_media_subtitles.py` でCPU上のmux処理だけを使って再生成できます。
映像・音声の生成条件と権利は元の合成動画と同じです。

## BML presentation fixtures

`bml/` contains hand-written BML, with no broadcast material. The startup
data-broadcast test sends it through a private WebSocket to the bundled web-bml.
The hidden startup document opens a top page on the data key. Enter opens a child
document whose external script is withheld until the test observes navigation;
Back returns to the top and then to standby. The test checks input and focus,
resuming the retained browser, a single activation per open request, Esc as BML Back,
and cleanup through the feature setting.
The hidden startup retains `basic data-button` to verify receiver navigation
while the engine is hidden, and focus restoration on redisplay even when the
requested key groups do not change.
`startup-visible.bml` starts with visible television video and still needs the data
key. It retains the default key mask without a focus/access-key target, reproducing
ABC's video-only page after closing with d. The test checks real Down input,
guide navigation, and reopening the same WebEngine view with d.
`startup-preload.bml` first locks a module, supplied after navigation finishes,
and launches that visible startup document from its ModuleLocked handler. These
cases check that activation waits for startup resource requests and navigation.
`startup-timer.bml` has no data-key subscriber and navigates after a timer; the
opening request must survive until the destination subscribes to DataButtonPressed.
`startup-initializing.bml` subscribes immediately but ignores input until its
300 ms initialization timer completes, reproducing another startup race.
The hidden-startup scenario first supplies a complete program identity with no BML
entry, checks the unavailable state, then adds an entry in the PMT and verifies
recovery without recreating the browser.
Presentation and geometry reach QML through the real Qt WebChannel. The final
scenario keeps the WebEngineView alive through application shutdown to exercise
delivery teardown as well as cleanup through the feature setting.
The top page has an opaque white background and a video window. Pixel samples
verify that the native moving-ball fixture remains visible through that window
after opening, navigating back and resuming from standby; geometry alone would
miss an opaque background covering the video.
Red moves the top page's video window and blue restores it. The test also resizes
the product window to verify geometry delivery through change notifications,
without periodic polling by the browser adapter.
The hidden-startup case adds authored green ARIB captions and a red fixed comment
to the production overlays. Pixel bounds verify that both appear inside the video
cutout after BML movement, window resizing and navigation, and return to the
full-size picture in standby. Bitmap-caption bounds use the same viewport.
`bml/transport.rs` generates PAT/PMT, DII and DDB packets from the authored
`overlay.bml`. Hardware-free decoder tests use those packets to check manual and
automatic entry detection and monitor-to-display transitions. The startup test
adds the same carousel to `recording-seek.ts`, exercising the production Rust
receiver and WebEngine together. The overlay changes its key groups on d/red
input and from a timer after d closes its menu; it checks automatic startup,
per-group keyboard routing, continued execution after Esc, and feature disablement.
The same test starts with the experimental feature disabled, including an
automatic-start carousel with prefetch saved as enabled. It checks that d and
direct open requests cannot start reception or a browser. Toggling the real
settings control enables reception on the existing playback connection; toggling
it off destroys the browser, releases its listening socket and restores player
input. Both settings values are checked on disk without changing user settings.
No generated TS is checked in.
Run `python3 scripts/test.py startup -- data-broadcast`; the runner validates the
private display, GPU and virtual audio output before starting the product window.
