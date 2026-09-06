pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls

Popup {
    id: popup
    required property var program
    required property point cellPosition
    required property string channelLabel
    property string watchError: ""
    property double now: Date.now()
    readonly property bool live: !!program && typeof program.watchKey === "string"
        && program.startAt <= now && now < program.startAt + program.duration
    signal watchRequested(string key)

    // Keep coordinates, not a delegate reference: offscreen columns are destroyed.
    width: Math.max(0, Math.min(500, parent.width - 48))
    height: Math.max(0, Math.min(360, parent.height - 40))
    x: Math.max(24, Math.min(parent.width - width - 24,
        cellPosition.x > parent.width / 2 ? cellPosition.x - width - 28 : cellPosition.x + 222 + 24))
    y: Math.max(20, Math.min(parent.height - height - 20, cellPosition.y))
    padding: 28
    margins: 0
    modal: false
    dim: false
    focus: true
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
    transformOrigin: Popup.Center
    enter: Transition {
        NumberAnimation { property: "opacity"; from: 0; to: 1; duration: 150; easing.type: Easing.OutCubic }
        NumberAnimation { property: "scale"; from: 0.97; to: 1; duration: 180; easing.type: Easing.OutBack }
    }
    exit: Transition {
        NumberAnimation { property: "opacity"; to: 0; duration: 150; easing.type: Easing.OutCubic }
        NumberAnimation { property: "scale"; to: 0.97; duration: 180; easing.type: Easing.OutBack }
    }
    Component.onCompleted: open()
    onAboutToShow: now = Date.now()
    Timer { interval: 1000; repeat: true; running: popup.opened; onTriggered: popup.now = Date.now() }
    background: Rectangle { radius: 18; color: "#151715"; border.color: "#b8ffffff" }
    contentItem: Column {
        id: body
        spacing: 14
        Label {
            id: timeLabel
            text: popup.program ? Qt.formatTime(new Date(popup.program.startAt), "hh:mm") + "–"
                + Qt.formatTime(new Date(popup.program.startAt + popup.program.duration), "hh:mm") : ""
            color: "#9caf9f"; font.bold: true
        }
        Label {
            id: titleLabel
            objectName: "programTitle"
            width: parent.width
            height: Math.min(implicitHeight, Math.max(font.pixelSize, popup.availableHeight
                - timeLabel.height - channelLabel.height - 1 - 4 - 5 * body.spacing
                - (popup.live ? 44 + body.spacing : 0)
                - (errorLabel.visible ? errorLabel.height + body.spacing : 0)))
            text: popup.program ? (popup.program.name || qsTranslate("Viewer", "No program information")) : ""
            textFormat: Text.PlainText
            color: "#e6e8e6"; font.pixelSize: 22; font.bold: true
            wrapMode: Text.Wrap; maximumLineCount: 3; elide: Text.ElideRight
        }
        Label {
            id: channelLabel
            objectName: "programChannel"
            width: parent.width
            text: popup.channelLabel; textFormat: Text.PlainText
            color: "#b6bab6"; elide: Text.ElideRight
        }
        Rectangle { width: parent.width; height: 1; color: "#20ffffff" }
        Label {
            objectName: "programDescription"
            width: parent.width
            // Preserve main's 88px description; shorten it for long titles or errors
            // so the watch action remains within the card.
            height: Math.max(0, Math.min(88, popup.availableHeight - timeLabel.height
                - titleLabel.height - channelLabel.height - 1 - 4
                - (popup.live ? 44 + body.spacing : 0) - 5 * body.spacing
                - (errorLabel.visible ? errorLabel.height + body.spacing : 0)))
            text: popup.program ? (popup.program.description || "") : ""
            textFormat: Text.PlainText; color: "#d9dcda"
            wrapMode: Text.Wrap; elide: Text.ElideRight; clip: true
        }
        Label {
            id: errorLabel
            objectName: "watchGuideError"
            visible: popup.watchError !== ""
            width: parent.width
            text: popup.watchError; textFormat: Text.PlainText
            wrapMode: Text.Wrap; color: "#ffb4ab"
        }
        Item { width: 1; height: 4 }
        Button {
            id: watchButton
            objectName: "watchGuideProgram"
            visible: popup.live
            width: 168; height: 44
            text: qsTranslate("Viewer", "Watch this program")
            background: Rectangle { radius: 22; color: "#9caf9f" }
            contentItem: Label {
                text: watchButton.text; color: "#17201a"; font.bold: true
                horizontalAlignment: Text.AlignHCenter; verticalAlignment: Text.AlignVCenter
            }
            onClicked: popup.watchRequested(popup.program.watchKey)
        }
    }
}
