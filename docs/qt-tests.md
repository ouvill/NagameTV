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
| `bash scripts/test-connection.sh` | Real Player and local HTTP fixtures: pending/failed connections preserve saved settings, empty catalogs, successful saves, save failures and shutdown during a request | None: QCoreApplication and HTTP |
| `bash scripts/test-subtitle-outline.sh` | Six pixel-exact QPainterPath/SVG comparisons: full height, small ink/cubic curves, midline, overhang/descender, separate contours, empty path | None: QCoreApplication and in-memory QImage rasterization |
| `bash scripts/test-pointer-activity.sh` | Duplicate installation, repeated positions, disabled items, window changes, observer deletion and event delivery after item destruction | Validated X11 display and GPU |
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

`rust/src/native_tests/qt_test_api.h` exposes resource registration, Qt object
introspection, raster drawing and event delivery. It contains no test cases or
assertions. Its raster helpers scope QPainter to the borrowed image so a native
paint-device pointer cannot escape into Rust. Pointer lifetime assertions use
a QPointer, while the Rust test owns its windows and item with UniquePtr.

The subtitle memory benchmark builds and copies the same Rust runner, including
its Rust outline provider. It preserves Qt Quick Test arguments and the stroke
setting. Its baseline includes the Rust application module and differs from the
former small C++ executable; compare memory measurements using the same runner.
