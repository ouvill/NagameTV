pragma Singleton
import QtQuick

// Why not Window.flags: QWindow::flags has no change notification before
// Qt 6.9 and the AppImage ships 6.8, so bindings on it never re-evaluate
// when the title bar setting changes at runtime. Main.qml derives both the
// window flags and the custom chrome from this one value.
QtObject {
    property bool frameless: true
    // Kiosk launches have no window to minimize, restore or close.
    property bool kiosk: false
}
