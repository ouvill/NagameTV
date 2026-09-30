pragma ComponentBehavior: Bound
import QtQuick
import MinimalViewer
import QtQuick.Controls

Rectangle {
    id: root
    required property Window targetWindow
    property url iconDirectory: "qrc:/qt/qml/MinimalViewer/assets/icons/"
    readonly property list<Item> navigationButtons: [minimizeButton, maximizeButton, closeButton]
    property bool flat: false
    property bool focusLabelBelow: false
    implicitWidth: Theme.iconButtonSize * 3
    implicitHeight: Theme.iconButtonSize
    radius: height / 2
    color: flat ? "transparent" : Theme.overlaySurface
    border.color: flat ? "transparent" : Theme.overlayBorder
    component Action: IconAction {
        required property string iconName
        required property string label
        iconSource: root.iconDirectory + iconName + ".svg"
        tip: label
        iconSize: Theme.smallIconSize
        flat: true
        focusLabelBelow: root.focusLabelBelow
    }
    Row {
        anchors.fill: parent
        Action {
            id: minimizeButton
            objectName: "minimizeWindow"
            iconName: "minus"
            label: qsTranslate("Viewer", "Minimize")
            onClicked: root.targetWindow.showMinimized()
        }
        Action {
            id: maximizeButton
            objectName: "maximizeWindow"
            iconName: "square"
            label: root.targetWindow.visibility === Window.Maximized ? qsTranslate("Viewer", "Restore window") : qsTranslate("Viewer", "Maximize")
            onClicked: root.targetWindow.visibility === Window.Maximized ? root.targetWindow.showNormal() : root.targetWindow.showMaximized()
        }
        Action {
            id: closeButton
            objectName: "closeWindow"
            iconName: "x"
            label: qsTranslate("Main", "Close")
            emphasis: IconAction.Destructive
            // Window.close delivers onClosing, including the player's shutdown.
            onClicked: root.targetWindow.close()
        }
    }
}
