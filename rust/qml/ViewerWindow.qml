pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls

ApplicationWindow {
    id: root
    enum Startup { Uninitialized, Ready }
    readonly property size referenceSize: Qt.size(1280, 720)
    readonly property real uiScale: Math.min(1, width / referenceSize.width, height / referenceSize.height)
    readonly property alias viewport: viewport
    default property alias viewportData: viewport.data

    width: referenceSize.width
    height: referenceSize.height
    minimumWidth: geometryState.minimumSize.width
    minimumHeight: geometryState.minimumSize.height

    function windowSizeToRemember() {
        // Read the current geometry at the close request. A cached value-type
        // binding can still contain the previous size during a native resize.
        return root.visibility === Window.Windowed
            ? Qt.size(root.width, root.height) : geometryState.normalSize;
    }

    function initializeWindow(options) {
        if (geometryState.phase !== ViewerWindow.Uninitialized || !options) return false;
        geometryState.minimumSize = Qt.size(options.minimum.width, options.minimum.height);
        width = options.initial.width;
        height = options.initial.height;
        geometryState.normalSize = Qt.size(width, height);
        geometryState.phase = ViewerWindow.Ready;
        show();
        return true;
    }

    onWidthChanged: if (geometryState.phase === ViewerWindow.Ready) rememberNormalSize.restart()
    onHeightChanged: if (geometryState.phase === ViewerWindow.Ready) rememberNormalSize.restart()
    onVisibilityChanged: if (geometryState.phase === ViewerWindow.Ready) rememberNormalSize.restart()

    // A fullscreen/maximized surface must not replace the last windowed size.
    // Coalesce native geometry/state notifications before taking the snapshot.
    contentData: [
        QtObject {
            id: geometryState
            property int phase: ViewerWindow.Uninitialized
            property size minimumSize: Qt.size(0, 0)
            property size normalSize: Qt.size(0, 0)
        },
        Timer {
            id: rememberNormalSize
            interval: 0
            onTriggered: {
                if (root.visibility === Window.Windowed)
                    geometryState.normalSize = Qt.size(root.width, root.height);
            }
        },
        Item {
            id: viewport
            width: root.width / root.uiScale
            height: root.height / root.uiScale
            scale: root.uiScale
            transformOrigin: Item.TopLeft
        }
    ]

    // Qt reparents Popup.Item content into the overlay. Keep its coordinates
    // and transform identical to the viewport, including full-window dialogs.
    // Anchors retain the logical bounds when Qt updates the overlay geometry.
    Overlay.overlay.anchors.fill: root.Overlay.overlay.parent
    Overlay.overlay.anchors.rightMargin: root.width - viewport.width
    Overlay.overlay.anchors.bottomMargin: root.height - viewport.height
    Overlay.overlay.transform: Scale {
        xScale: root.uiScale
        yScale: root.uiScale
    }
}
