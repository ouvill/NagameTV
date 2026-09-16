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

| Command | Preserved coverage | Hardware |
| --- | --- | --- |
| `bash scripts/test-localization.sh` | Locale resolution, existing QML retranslation, date stability, dynamic snapshots, literal diagnostic arguments, 100 repeated language switches and translator ownership; a separate process removes the real catalog resource and checks failure cleanup | None: QCoreApplication and QtObject |
| `bash scripts/test-connection.sh` | Real Player and local HTTP fixtures: pending/failed saves, empty catalogs, save failures, shutdown, coherent stream properties during Qt signals, retry allowance and guide visibility/day notification order | None: QCoreApplication and HTTP |
| `bash scripts/test-startup.sh` | Production Main.qml in separate processes: first run, two saved startups, channel restoration, guide open/close, native playback failure and clean shutdown; QML warnings fail the test | Validated X11 display, GPU and PulseAudio output |
| `bash scripts/test-screenshot.sh` | Real Player and item capture, instant PNG saving, overlay pixels, exclusion of sibling controls, repeated captures, Unicode/escaped folder names, cancellation and invalid folder rejection | Validated X11 display and GPU |
| `bash scripts/test-video-item.sh` | Video-item attachment, terminal shutdown, failed native transitions, and retained subtitle subscriptions until a successful stop | Validated X11 display and GPU; native graph stays in NULL |
| `bash scripts/test-subtitle-outline.sh` | Six pixel-exact QPainterPath/SVG comparisons: full height, small ink/cubic curves, midline, overhang/descender, separate contours, empty path | None: QCoreApplication and in-memory QImage rasterization |
| `bash scripts/test-pointer-activity.sh` | Duplicate installation, repeated positions, disabled items, window changes, observer deletion and event delivery after item destruction | Validated X11 display and GPU |
| `bash scripts/test-portal-dialogs.sh` | Real Qt portal plugin on a private D-Bus session: file/folder selection, cancellation and Unicode/escaped URLs | Validated X11 display and GPU; no audio |
| `bash scripts/test-subtitle-rendering.sh` | Existing Qt Quick Test assertions, using the Rust TestOutlineProvider and the production outline helper | Validated X11 display and GPU |

The subtitle rendering command continues to accept Qt Quick Test arguments
after its optional `--ui-only` flag. The other suites run all Rust assertions;
they do not accept QtTest function filters. `--ui-only` remains an explicit
software-rendering choice, never a fallback after a hardware check fails.

Build without accessing hardware:

```sh
cargo build --manifest-path rust/Cargo.toml --locked --features native_tests
```

Use the validating scripts for hardware-dependent execution.
`scripts/run-native-tests.sh` is their common Cargo launcher.

The startup suite uses an isolated configuration and a local HTTP fixture, with
the real Player, GStreamer pipeline and video item. The fixture returns HTTP 503
for a stream request to exercise failure cleanup without a tuner. It does not
verify successful broadcast playback or audible sound. The internal
`startup-window` subprocess is launched by the suite after hardware validation;
run the public script rather than invoking this subprocess directly.

The startup suite also covers persisted autoplay and both directions of the
environment override, using the restored channel. Screenshot tests copy their
fixture and production component into a temporary directory and run the native
test runner with the real Player; saved images are removed at exit. They use a
rendered rectangle and child overlay to verify the capture contract. Successful
capture of broadcast video still requires manual verification with a playing
stream, including visible subtitles and comments.

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
appearance of GNOME/KDE's chooser or Flatpak document grants. It does not use a
headless or software renderer as a substitute for missing display hardware.

Recording inspection is asynchronous: component tests cover completion, failure
and cancellation UI; connection tests verify input/activity coherence during
every related Qt notification. Hardware-free worker tests control completion to
check that cancelled and superseded results cannot start playback, and that a
replacement never overlaps its predecessor's filesystem worker.

Hardware-free Rust tests verify screenshot directory serialization, atomic PNG
publication, filename collisions and cleanup. Connection tests also verify the
real Qt properties, folder persistence, PNG saving and recovery after the folder
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
