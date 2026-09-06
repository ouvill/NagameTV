pragma ComponentBehavior: Bound
import QtQuick

Item {
    id: frame
    required property Window targetWindow
    function resize(edges) {
        if (!targetWindow.startSystemResize(edges))
            console.warn("Could not start the system window resize");
    }
    component ResizeEdge: MouseArea {
        required property int edges
        enabled: frame.targetWindow.visibility === Window.Windowed
        acceptedButtons: Qt.LeftButton
        z: 1000
        onPressed: frame.resize(edges)
    }
    ResizeEdge {
        edges: Qt.LeftEdge
        anchors {
            left: parent.left
            top: parent.top
            bottom: parent.bottom
        }
        width: 10
        cursorShape: Qt.SizeHorCursor
    }
    ResizeEdge {
        edges: Qt.RightEdge
        anchors {
            right: parent.right
            top: parent.top
            bottom: parent.bottom
        }
        width: 10
        cursorShape: Qt.SizeHorCursor
    }
    ResizeEdge {
        edges: Qt.TopEdge
        anchors {
            left: parent.left
            right: parent.right
            top: parent.top
        }
        height: 10
        cursorShape: Qt.SizeVerCursor
    }
    ResizeEdge {
        edges: Qt.BottomEdge
        anchors {
            left: parent.left
            right: parent.right
            bottom: parent.bottom
        }
        height: 10
        cursorShape: Qt.SizeVerCursor
    }
    ResizeEdge {
        edges: Qt.LeftEdge | Qt.TopEdge
        anchors {
            left: parent.left
            top: parent.top
        }
        width: 18
        height: 18
        z: 1001
        cursorShape: Qt.SizeFDiagCursor
    }
    ResizeEdge {
        edges: Qt.RightEdge | Qt.TopEdge
        anchors {
            right: parent.right
            top: parent.top
        }
        width: 18
        height: 18
        z: 1001
        cursorShape: Qt.SizeBDiagCursor
    }
    ResizeEdge {
        edges: Qt.LeftEdge | Qt.BottomEdge
        anchors {
            left: parent.left
            bottom: parent.bottom
        }
        width: 18
        height: 18
        z: 1001
        cursorShape: Qt.SizeBDiagCursor
    }
    ResizeEdge {
        edges: Qt.RightEdge | Qt.BottomEdge
        anchors {
            right: parent.right
            bottom: parent.bottom
        }
        width: 18
        height: 18
        z: 1001
        cursorShape: Qt.SizeFDiagCursor
    }
}
