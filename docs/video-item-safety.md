# Video item FFI safety

`Player::attach` accepts a QML `QQuickItem*`, but `qml6glsink` does not accept
arbitrary Qt Quick items. `Playback::attach` validates the pointer through
`qml6VideoItemPointer` immediately before setting `widget`. Null, incompatible
items, and calls outside the application GUI thread are rejected without
changing the sink property. No pointer is converted through an integer.

An existing attachment is rejected before accessing the supplied pointer, even
when the caller supplies the same item. The plugin's widget setter and streaming
callback access the same shared-pointer instance without synchronization.
GUI-thread validation alone does not make replacement during playback safe.
Shutdown is terminal for a `Playback`: a new attachment requires a new instance.
This also avoids reusing playbin after NULL reset its stream-synchronizer state
while other elements may still retain request pads. Ordinary channel changes
continue to use READY via `stop_stream`.

Shutdown transitions playbin and the sink to NULL, checks their completed states,
and only then clears `widget`. A failed transition preserves the attachment;
`Player::shutdown` returns false and `Main.qml` rejects the close event so the
owner stays alive and shutdown can be retried. NULL shutdown goes directly through
GStreamer's downward transitions; it does not depend on a separate READY stop.
Once shutdown begins, new playback and attachment requests are rejected even if
a native transition fails; a partially stopped graph must not be reused.
See [GStreamer state transitions](https://gstreamer.freedesktop.org/documentation/additional/design/states.html).

During final Rust/QML destruction there is no close event left to reject. If a
shutdown retry fails there, the process aborts rather than unwinding through CXX
or releasing a potentially live graph. This is a last-resort failure policy, not
a guarantee that native driver failures are recoverable. Native calls that hang
also cannot be bounded by this state check.

## Checked upstream contracts

- [Qt QObject inheritance](https://doc.qt.io/qt-6/qobject.html#inherits) includes
  subclasses. [Qt's generated `qt_metacast`](https://github.com/qt/qtbase/blob/v6.10.3/src/tools/moc/generator.cpp#L493) walks this same meta-object class
  hierarchy and returns the adjusted base pointer or null. This admits QML
  subclasses of the genuine video item, unlike comparing `className()`.
- [Qt thread rules](https://doc.qt.io/qt-6/threads-qobject.html) require GUI work
  on the main thread. The helper checks the application thread before touching
  the item and then checks its thread affinity.
- [GStreamer 1.28.2 widget setter](https://github.com/GStreamer/gstreamer/blob/1.28.2/subprojects/gst-plugins-good/ext/qt6/gstqml6glsink.cc#L215)
  casts `gpointer` directly to `Qt6GLVideoItem*` and calls `getInterface()`.
  The required native class has `Q_OBJECT` and QQuickItem as its first base in
  [qt6glitem.h](https://github.com/GStreamer/gstreamer/blob/1.28.2/subprojects/gst-plugins-good/ext/qt6/qt6glitem.h).
  The helper uses the native meta-cast result, not the original unchecked pointer.
- [Item destruction](https://github.com/GStreamer/gstreamer/blob/1.28.2/subprojects/gst-plugins-good/ext/qt6/qt6glitem.cc#L134)
  invalidates the shared interface under its mutex before freeing item data.
  The sink does not take ownership of the QML item. The application retains its
  conservative contract: shutdown stops the sink and clears `widget` before the
  QML owner destroys the item (`Main.qml`'s closing handler).

The plugin's C++ item header is private, so this check uses its Qt meta-object
rather than copying a C++ class declaration or linking against its private ABI.
A changed native class name fails closed and must be reviewed on plugin upgrades.
The application trusts its installed native plugins and their Qt meta-objects;
this is not protection against a native plugin impersonating that class.

## Remaining unsafe obligations

A type check cannot validate a dangling address. Rust callers must pass null or
an actual live QQuickItem, keep it alive while the function executes, and obey
GUI-thread and teardown requirements documented in both `attach` functions.
The returned `u8*` is only an opaque transport for the CXX bridge; Rust never
reads bytes through it. There is no event-loop dispatch between validation and
the GStreamer setter.

## Regression verification

Run `scripts/test-video-item.sh`. It checks null, an ordinary QQuickItem, a real
GStreamer video item with a QML-generated subclass, worker-thread rejection,
and the actual sink's widget setter/getter and detach before destruction.
It also constructs production `Playback` and exercises its attachment, duplicate
rejection (including absence of property writes), shutdown, repeated shutdown,
and rejection of attachment after shutdown. Injected failures at each native stop step must
preserve both the property and attachment state and continue to reject reattach.
The script requires and validates a real X11 display and GPU, following the
Workshop rules. The graph remains in NULL: these are lifecycle/error-path checks,
not a streaming stress test or a test of real driver failure. No audio is used.
Compilation alone
does not require those resources:

```sh
cargo build --manifest-path rust/Cargo.toml --locked --features video_item_tests
```

The checks live in `rust/src/video_item_tests.rs`, using gstreamer-rs and a
cxx-qt-generated test QObject. `tests/video_item.qml` only constructs the real
items. The feature-gated runner executes on the process main thread, catches
assertion failures before returning through Qt, and fails if the QML fixture
does not run. It requires no handwritten C++ test class or QtTest dependency.

Prefer Rust for application logic and test assertions, cxx-qt for QObject glue,
and QML for UI fixtures. Keep handwritten C++ limited to Qt APIs that cannot be
expressed through the existing bindings; moving unsafe operations into C++ is
not by itself a safety improvement.
