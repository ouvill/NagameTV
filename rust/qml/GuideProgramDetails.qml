pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Popup {
    id: popup
    required property var program
    required property point cellPosition
    required property real channelWidth
    required property string channelLabel
    property url iconDirectory: "qrc:/qt/qml/MinimalViewer/assets/icons/"
    property string uiLanguage: Qt.uiLanguage
    // Backend translation source; retranslate even while the error remains visible.
    property string watchError: ""
    property double now: Date.now()
    readonly property bool live: !!program && typeof program.watchKey === "string"
        && program.startAt <= now && now < program.startAt + program.duration
    signal watchRequested(string key)

    // Keep coordinates, not a delegate reference: offscreen columns are destroyed.
    width: Math.max(0, Math.min(560, parent.width - 48))
    height: Math.max(0, Math.min(620, parent.height - 40))
    x: Math.max(24, Math.min(parent.width - width - 24,
        cellPosition.x > parent.width / 2 ? cellPosition.x - width - 28 : cellPosition.x + channelWidth + 24))
    y: Math.max(20, Math.min(parent.height - height - 20, cellPosition.y))
    padding: 24
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
    background: Rectangle { radius: 18; color: "#151c17"; border.color: "#627c69" }
    contentItem: ColumnLayout {
        spacing: 16
        RowLayout {
            Layout.fillWidth: true
            Label {
                text: qsTranslate("Viewer", "Program details")
                color: "#b7cdbd"; font.pixelSize: 13
                Layout.fillWidth: true
            }
            ToolButton {
                id: closeButton
                objectName: "closeGuideProgramDetails"
                Layout.preferredWidth: 32; Layout.preferredHeight: 32
                Accessible.name: qsTranslate("Main", "Close")
                background: Rectangle {
                    radius: 8
                    color: closeButton.down ? "#354b3c" : closeButton.hovered ? "#293a2f" : "transparent"
                    border.width: closeButton.visualFocus ? 1 : 0
                    border.color: "#9caf9f"
                    Behavior on color { ColorAnimation { duration: 100 } }
                }
                contentItem: Image { source: popup.iconDirectory + "x.svg"; sourceSize: Qt.size(16, 16); fillMode: Image.Pad }
                onClicked: popup.close()
            }
        }
        ScrollView {
            id: detailsScroll
            Layout.fillWidth: true
            Layout.fillHeight: true
            rightPadding: 12
            clip: true
            focus: true
            ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
            ScrollBar.vertical.policy: detailsFlick.contentHeight > detailsFlick.height
                ? ScrollBar.AlwaysOn : ScrollBar.AlwaysOff
            ScrollBar.vertical.palette.mid: "#8c918c"
            contentItem: Flickable {
                id: detailsFlick
                objectName: "programDetailsFlickable"
                contentWidth: width
                contentHeight: body.implicitHeight
                boundsBehavior: Flickable.StopAtBounds
                flickableDirection: Flickable.VerticalFlick
                Column {
                    id: body
                    width: detailsFlick.width
                    spacing: 18
                    Label {
                        objectName: "programDateTime"
                        width: parent.width
                        text: {
                            if (!popup.program) return ""
                            const start = new Date(popup.program.startAt)
                            const end = new Date(popup.program.startAt + popup.program.duration)
                            const format = qsTranslate("Viewer", "ddd, MMM d · hh:mm")
                            const locale = Qt.locale(popup.uiLanguage)
                            return start.toLocaleString(locale, format) + " – "
                                + end.toLocaleString(locale, start.toDateString() === end.toDateString() ? "hh:mm" : format)
                        }
                        color: "#b7cdbd"; font.pixelSize: 13
                        wrapMode: Text.Wrap
                    }
                    Label {
                        objectName: "programTitle"
                        width: parent.width
                        text: popup.program ? (popup.program.name || qsTranslate("Viewer", "No program information")) : ""
                        textFormat: Text.PlainText
                        color: "#e6e8e6"; font.pixelSize: 22; font.bold: true
                        wrapMode: Text.Wrap; lineHeight: 1.15
                    }
                    Label {
                        objectName: "programChannel"
                        width: parent.width
                        text: popup.channelLabel; textFormat: Text.PlainText
                        color: "#b6bab6"; wrapMode: Text.Wrap
                    }
                    ProgramFacts {
                        objectName: "programFacts"
                        width: parent.width
                        program: popup.program
                        now: popup.now
                    }
                    Rectangle { width: parent.width; height: 1; color: "#20ffffff" }
                    Label {
                        objectName: "programDescription"
                        width: parent.width
                        text: popup.program ? (popup.program.description || "") : ""
                        textFormat: Text.PlainText; color: "#d9dcda"
                        visible: text.length > 0
                        wrapMode: Text.Wrap; font.pixelSize: 14; lineHeight: 1.25
                    }
                    ProgramMetadata {
                        objectName: "programMetadata"
                        width: parent.width
                        program: popup.program
                        visible: implicitHeight > 0
                    }
                    Label {
                        objectName: "watchGuideError"
                        visible: popup.watchError !== ""
                        width: parent.width
                        text: popup.watchError ? qsTranslate("Backend", popup.watchError) : ""; textFormat: Text.PlainText
                        wrapMode: Text.Wrap; color: "#ffb4ab"
                        onTextChanged: function(text) { if (text) revealError.restart() }
                    }
                }
                // Wait for wrapped error text to settle before revealing its last line.
                Timer {
                    id: revealError
                    interval: 0
                    onTriggered: {
                        body.forceLayout()
                        detailsFlick.contentY = Math.max(0, detailsFlick.contentHeight - detailsFlick.height)
                    }
                }
            }
        }
        Button {
            id: watchButton
            objectName: "watchGuideProgram"
            visible: popup.live
            Layout.preferredWidth: 168
            Layout.preferredHeight: 44
            text: qsTranslate("Viewer", "Watch this program")
            background: Rectangle {
                radius: 12
                color: watchButton.down ? "#8eae99" : watchButton.hovered ? "#bdd6c4" : "#a8c4b0"
                border.width: watchButton.visualFocus ? 2 : 0
                border.color: "#e0f0e5"
                Behavior on color { ColorAnimation { duration: 100 } }
            }
            contentItem: Label {
                text: watchButton.text; color: "#17201a"; font.bold: true
                horizontalAlignment: Text.AlignHCenter; verticalAlignment: Text.AlignVCenter
            }
            onClicked: popup.watchRequested(popup.program.watchKey)
        }
    }
}
