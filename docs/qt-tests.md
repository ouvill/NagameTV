# Qt integration tests in Rust

Application logic and test scenarios belong in Rust. Use cxx-qt to generate
QObject classes, gstreamer-rs to operate GStreamer, and QML to construct UI
fixtures. Handwritten C++ is limited to small bindings for APIs not exposed by
cxx-qt-lib; moving assertions or unsafe operations into C++ is not a safety fix.

The `native_tests` development feature enables the Rust runners below. They run
on the process main thread instead of a libtest worker, and are excluded from
normal application builds. Test QObjects are generated from
`rust/src/native_test_bridge.rs`. Assertions and test inputs live in
`rust/src/native_tests/`; no standalone C++ test executables are needed.

GUI scripts automatically start an [isolated test session](gui-test-environment.md):
GPU-rendered Weston headless, private rootful Xwayland/Openbox, a private D-Bus session and
per-run clocked virtual output on an already running PipeWire Pulse server.
The environment owns the audio server; tests only own their virtual outputs.
Workshop starts the audio services in its SDK hook; native Linux can use the
existing local PipeWire server. Every session validates the display, GPU and
audio loopback before running its command, including suites that do not use audio.
Host desktop connections and physical audio devices are not required.

| Command | Preserved coverage | Hardware |
| --- | --- | --- |
| `bash scripts/test-localization.sh` | Locale resolution, existing QML retranslation, date stability, dynamic snapshots, literal diagnostic arguments, 100 repeated language switches and translator ownership; a separate process removes the real catalog resource and checks failure cleanup | None: QCoreApplication and QtObject |
| `bash scripts/test-connection.sh` | Real Player and local HTTP fixtures: pending/failed saves, empty catalogs, save failures, shutdown, coherent stream properties during Qt signals, retry allowance and guide visibility/day notification order; channel source/proxy models with QAbstractItemModelTester, exact 64-bit service IDs, filtering, replacement and source destruction | None: QCoreApplication and HTTP |
| `bash scripts/test-danmaku.sh` | QML component tests in `rust/qml/tests/`, including channel views using the production Rust models | Validated private X11 display and GPU |
| `bash scripts/test-ui-style.sh` | Production control catalogue, press/release hit areas, disabled actions, keyboard input, popup dismissal | Validated private X11 display and GPU |
| `bash scripts/test-channel-wheel.sh` | Channel browser and sidebar wheel input, cursor movement, snapping and filtering using the production Rust models | Validated private X11 display and GPU |
| `bash scripts/test-startup.sh` | Production Main.qml in separate processes: first run, two saved startups, channel restoration, guide open/close, native playback failure and clean shutdown; QML warnings fail the test | Validated private X11 display, GPU and virtual PipeWire output |
| `bash scripts/test-screenshot.sh` | Real Player, parallel PNG/JPG/WebP saving, immutable images/settings, accepted saves surviving UI unavailability, Unicode/escaped folders and failure recovery | Validated X11 display and GPU |
| `bash scripts/test-startup.sh screenshot-playback` | Production Main.qml, native 1080p numbered frames, captions and comments, visibility, resize/fullscreen, pixel aspect ratio, burst capture, seek/stop and frame interval measurements | Validated private X11 display, GPU and virtual PipeWire output |
| `bash scripts/test-video-item.sh` | Video-item attachment, terminal shutdown, failed native transitions, and retained subtitle subscriptions until a successful stop | Validated X11 display and GPU; native graph stays in NULL |
| `bash scripts/test-desktop-media.sh` | Linux MPRIS wire types, metadata, commands, property/seek signals, stale seeks, rejected values and registration cleanup on private D-Bus | None: QCoreApplication and synthetic metadata |
| `bash scripts/test-subtitle-outline.sh` | Six pixel-exact QPainterPath/SVG comparisons: full height, small ink/cubic curves, midline, overhang/descender, separate contours, empty path | None: QCoreApplication and in-memory QImage rasterization |
| `bash scripts/test-pointer-activity.sh` | Duplicate installation, repeated positions, disabled items, window changes, observer deletion and event delivery after item destruction | Validated X11 display and GPU |
| `bash scripts/test-portal-dialogs.sh` | Real Qt portal plugin on a private D-Bus session: file/folder selection, FileTransfer TS/M2TS inspection, invalid transfers, Unicode paths and production QML drop areas including first-run setup | Validated X11 display and GPU; the suite itself does not use audio |
| `bash scripts/test-subtitle-rendering.sh` | Existing Qt Quick Test assertions, using the Rust TestOutlineProvider and the production outline helper | Validated X11 display and GPU |

The subtitle rendering command continues to accept Qt Quick Test arguments
after its optional `--ui-only` flag. The other suites run all Rust assertions;
they do not accept QtTest function filters. `--ui-only` remains an explicit
software-rendering choice using a separately provisioned test display; it does
not automatically start an isolated session or fall back after a GPU check fails.

Build without accessing hardware:

```sh
bash scripts/with-build-lock.sh cargo build --manifest-path rust/Cargo.toml --locked --features native_tests
```

Use the validating scripts for hardware-dependent execution.
`python3 scripts/test.py native` builds once and runs the hardware-free Qt suites.
`python3 scripts/test.py gui` runs the listed GUI suites, after resource validation.
The existing shell entry points remain available and share the build/test lock.
Their common launcher uses a private executable copy and defaults to `release`;
set `NAGAMETV_TEST_PROFILE=dev` for standalone scripts or `--profile dev` on the
common runner. Only `dev` and `release` are accepted.

`bash scripts/capture-ui-style.sh` separately generates regular/small-size,
focus, hover and popup images in `build/ui-review/` for human review.
These images are not compared automatically and are not counted as visual regression coverage.

The startup suite uses an isolated configuration and a local HTTP fixture, with
the real Player, GStreamer pipeline and video item. The fixture returns HTTP 503
for a stream request to exercise failure cleanup without a tuner. It does not
verify successful broadcast playback or audible sound. The internal
`startup-window` subprocess is launched by the suite after hardware validation;
run the public script rather than invoking this subprocess directly.

The [startup HTTP fixture](../rust/src/native_tests/startup/server.rs) uses wiremock
to read complete request bodies and isolate connections. Its hardware-free Rust
regressions run with the ordinary `cargo test` command; the `epgstation` filter
selects them alongside the catalogue tests. They cover split login bodies,
cancelled connections and repeated authentication beside an incomplete request.
Native runners initialize terminal logging, and the startup suite checks that a
rejected login reports an authentication failure as soon as the request finishes.
The EPGStation catalogue, channels and video metadata come from
[captured real-provider responses](../tests/fixtures/epgstation/README.md).
The separate hardware-free `python3 scripts/epgstation-integration.py` suite runs
the pinned EPGStation HTTP service in Docker and checks these snapshots and the
mock's file responses against it. GUI success alone does not verify a real server.

The startup suite also covers persisted autoplay and both directions of the
environment override, using the restored channel. Screenshot tests copy their
fixture and production component into a temporary directory and run the native
test runner with the real Player; saved images are removed at exit. They use a
rendered rectangle as a fixed image input for the queue tests. The production
capture path is independently exercised by `test-startup.sh screenshot-playback`:
a CPU-generated, numbered MPEG-2 video is played through the real GL sink and
compared to Qt's displayed frame. It checks authored subtitle/comment overlays,
clipping, hidden layers, frozen burst captures, resize/fullscreen and 4:3/non-square
pixels. The test records frameSwapped intervals on the render thread, request
latency, save completion, file sizes and Linux CPU/RSS observations in
`build/screenshot-review/metrics.json`. Review images are written alongside it;
its TS input and saved test images use temporary directories. Run separately
from builds or other GPU tests when comparing performance. `QT_SCALE_FACTOR=1.5`
with the same public command covers high-DPI rendering. These synthetic tests do
not verify a real tuner, broadcast-specific font rendering or physical display latency.

Startup also checks the shared top-right navigation at 640×360, 960×540,
1280×720 and 1440×810, including scaled popup alignment,
settings-to-guide/live transitions, recording-picker cancellation from settings,
and guide open/close with the viewing sidebar retained but hidden in the guide.
Review images for viewing, guide and settings are saved to `build/navigation-review/`.
It verifies the initial 16:9 size and restores a saved, freely resized 850×610
window across processes. Hardware-free preference tests cover smaller logical
work areas, old settings without dimensions, invalid sizes and persistence;
QML tests cover keeping the windowed size through maximize/fullscreen transitions.

Local recording coverage in the startup suite drops a generated MPEG-2/AAC TS
onto the production window, including its first-run setup screen. It also opens
the recording picker through mode navigation, cancels without changing playback,
then reopens and accepts a Unicode/escaped filename using the startup dialog policy. It verifies
rendered video frames, an audio track, no live program/comment association,
stop/replay, normal EOF, file removal, cancellation and switching back to a live
channel. The audio fixture is silent. This does not verify real broadcast
captions or Flatpak portal access. Component tests cover the file-open actions,
single-file URL handling and rejected input.
The suite also plays `recording-clock-reset.ts` across a PCR/PTS reset and
`recording-pid-change.ts` across a same-service PMT/video/audio PID change.
Each requires at least 140 observed rendering calls out of 150 frames and normal
EOF within 12 seconds. Sink counters can reset when streams change, so the test
sums sampled increments across resets instead of treating the final counter as
a file total. Sampling can miss the final increments before a reset; the threshold
allows a small margin but cannot pass if only one 75-frame half plays.

`bash scripts/test-startup.sh recording-pid-change` runs the PID case alone
with the same hardware validation and isolated settings. Failures print playback
state and video/audio diagnostics. The earlier reported stall was a false test
failure caused by reset counters, corrected on 2026-09-17. Position continuity and
seeking across PID changes remain separate validation work. See the
[recording verification record](recording-seek-verification.md#代表ケースの信頼性検証2026-09-16).

`bash scripts/test-startup.sh recording-audit PATH` compares a six-second recovery
fixture through the production window. It logs position and audio selection every
100 ms, then checks paused seeks to 4500, 1000 and 4500 ms (500 ms tolerance),
resumes and requires rendered output and normal EOF. Position decreases and audio
selection are diagnostic observations, not asserted by the exit status. This is
not a general-purpose long-recording test. The same hardware validation applies.
See the [tsreadex comparison](tsreadex-trial.md) for inputs, CPU output counting,
real audio sink diagnostics and known failures in the original input.

GTK warnings and criticals also fail the startup suite. To check the native Linux
fallback, run `bash scripts/test-startup.sh` in a session without a FileChooser
portal. Production startup selects Qt Quick dialogs in this case.

The portal dialog suite requires Qt's `xdgdesktopportal` platform theme,
`dbus-run-session`, Python `dbus` and PyGObject. It starts a private D-Bus
protocol fixture; it never replaces the user's desktop portal. The real Qt
plugin receives accept/cancel responses from that fixture and passes file URLs
through the ordinary QML dialogs. This checks protocol integration, not the
appearance of GNOME/KDE's chooser or Flatpak document grants. Its private display
is explicitly configured for GPU rendering; a failed GPU check stops execution.
The same private service exercises FileTransfer on the Documents bus name and
path. Tests prefer transfer keys over inaccessible host URLs, accept portal-only
drops and GTK's legacy MIME name, and reject consumed keys, empty/multiple files
and relative paths. Transfer keys resolve before drop acknowledgement because
KDE senders close their transfer when the drag ends. These protocol fixtures do
not verify document FUSE mounts or a particular host file manager.

Recording inspection is asynchronous: component tests cover completion, failure
and cancellation UI; connection tests verify input/activity coherence during
every related Qt notification. Hardware-free worker tests control completion to
check that cancelled and superseded results cannot start playback, and that a
replacement never overlaps its predecessor's filesystem worker.

Hardware-free Rust tests verify screenshot directory serialization, atomic PNG/JPG/WebP
publication, filename collisions, encoding parameters, lossless pixels, queue bounds,
reverse completion order, partial failure and cleanup. Presentation tests distinguish
synchronized from presented frames and reject buffers from an old seek generation. Connection tests also verify the
real Qt properties, folder/format persistence, asynchronous saving and recovery after the folder
becomes inaccessible. These checks only rasterize in-memory QImages and use
temporary files; they do not create a display or use a GPU.

`rust/src/native_tests/qt_test_api.h` exposes resource registration, Qt object
introspection, raster drawing and event delivery. It contains no test cases or
assertions. Its raster helpers scope QPainter to the borrowed image so a native
paint-device pointer cannot escape into Rust. Pointer lifetime assertions use
a QPointer, while the Rust test owns its windows and item with UniquePtr.

The subtitle memory benchmark builds and copies the same Rust runner, including
its Rust outline provider. It preserves Qt Quick Test arguments and the stroke
setting. Its baseline includes the Rust application module and differs from the
former small C++ executable; compare memory measurements using the same runner.

`bash scripts/test-startup.sh timeshift` uses a paced local HTTP TS source and
production Main.qml with the validated display/GPU/audio. It tests both memory
and filesystem retention: the window advances while paused, a backward seek
retains pause, return-to-live resumes playback, and stop releases the session.
The startup suite also includes these cases. Tests isolate XDG cache as well as
settings; they do not connect to a real tuner. Rust memory-sink tests independently
check raw SI at the playhead, TS/M2TS/204-byte framing, both store contracts,
anonymous file lifetime (including a subprocess killed with SIGKILL), disk-block
reclamation and slot reuse, cleanup of legacy disk sessions without deleting live
owners, sequential discontinuities and paused boundary seeks. The product test
observes the anonymous file descriptor during playback and its release after stop;
an empty directory alone is not treated as proof of resource release.

`bash scripts/test-startup.sh recording-probe PATH` accepts a longer real broadcast
recording. It checks output progress through 28 seconds (six seconds without new
rendering fails), paused seeks to 20 and 10 seconds, and a seek to 80% for files
longer than one minute. It logs the provisional duration and audio selection.
This uses the same GPU and virtual audio checks; it is not a full-file playback or audible
quality test. The timeshift suite also opens the production settings page, applies
custom budgets, and enables/disables retention during playback and pause.
Connection tests verify atomic budget notifications, rejected invalid values and
persistence without using display or audio hardware.

The timeshift settings component also checks mutually exclusive capacity controls,
retention estimates from received bytes and the time limit, automatic numeric saves,
coalesced repeated steps, flushing edits when leaving the page, failed-change rollback,
and storage changes that preserve both budgets. The product timeshift suite checks
that budget edits preserve paused playback and the HTTP connection, shrinking budgets
moves an expired paused position without resuming, and storage changes return to live
without reconnecting. Hardware-free tests cover capacity/time trimming, failed or
cancelled storage preparation, and paused position correction. Timeline component tests cover
the shared program/history axis, non-seekable future and expired portions, retained
data preceding the current program, and progress with retention disabled. CPU Rust
tests cover TS clock-to-program axis mapping and bitrate measurement across clock
resets. Subtitle clock tests reproduce replacement-video pad addition before old
pad removal without starting a decoder or using any output device.

Timeshift regressions additionally keep normal playback running beyond the disabled
forward buffer's lifetime, including disabling while paused. Valid null packets
raise the fixture bitrate so 16 MiB memory/filesystem budgets expire during pause;
resuming must then advance for four seconds without entering another seek. The CPU
test continues receiving after recovery and rejects stale expiry feedback from a
previous seek generation. Timeline tests check exact vertical centers at rest and
on hover, program boundary placement/RTL and removal outside the retained window.
Localization tests switch the actual compiled catalog between Japanese and English
for the history tooltip, storage descriptions and duration estimate.
