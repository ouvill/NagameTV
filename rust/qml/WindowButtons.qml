pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls

Rectangle {
    id: root
    required property Window targetWindow
    property url iconDirectory: "qrc:/qt/qml/MinimalViewer/assets/icons/"
    implicitWidth: 126
    implicitHeight: 42
    radius: 21
    color: "#b8171819"
    border.color: "#16ffffff"
    component Action: ToolButton {
        id: action
        required property string iconName
        required property string label
        property bool destructive: false
        implicitWidth: 42
        implicitHeight: 42
        Accessible.name: label
        background: Rectangle {
            radius: 21
            color: action.hovered ? (action.destructive ? "#a94b3f" : "#28ffffff") : "transparent"
            border.width: action.visualFocus ? 1 : 0
            border.color: "#9caf9f"
            Behavior on color {
                ColorAnimation {
                    duration: 100
                }
            }
        }
        contentItem: Item {
            Image {
                anchors.centerIn: parent
                width: 16
                height: 16
                sourceSize: Qt.size(16, 16)
                source: root.iconDirectory + action.iconName + ".svg"
            }
        }
    }
    Row {
        anchors.fill: parent
        Action {
            objectName: "minimizeWindow"
            iconName: "minus"
            label: qsTranslate("Viewer", "Minimize")
            onClicked: root.targetWindow.showMinimized()
        }
        Action {
            objectName: "maximizeWindow"
            iconName: "square"
            label: root.targetWindow.visibility === Window.Maximized ? qsTranslate("Viewer", "Restore window") : qsTranslate("Viewer", "Maximize")
            onClicked: root.targetWindow.visibility === Window.Maximized ? root.targetWindow.showNormal() : root.targetWindow.showMaximized()
        }
        Action {
            objectName: "closeWindow"
            iconName: "x"
            label: qsTranslate("Main", "Close")
            destructive: true
            // Window.close delivers onClosing, including the player's shutdown.
            onClicked: root.targetWindow.close()
        }
    }
}
