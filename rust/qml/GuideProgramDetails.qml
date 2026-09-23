pragma ComponentBehavior: Bound
import QtQuick
import MinimalViewer
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
    padding: Theme.spaceXl
    margins: 0
    modal: false
    dim: false
    focus: true
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
    transformOrigin: Popup.Center
    enter: Transition {
        NumberAnimation { property: "opacity"; from: 0; to: 1; duration: Theme.moveDuration; easing.type: Easing.OutCubic }
        NumberAnimation { property: "scale"; from: 0.97; to: 1; duration: Theme.moveDuration; easing.type: Easing.OutBack }
    }
    exit: Transition {
        NumberAnimation { property: "opacity"; to: 0; duration: Theme.moveDuration; easing.type: Easing.OutCubic }
        NumberAnimation { property: "scale"; to: 0.97; duration: Theme.moveDuration; easing.type: Easing.OutBack }
    }
    Component.onCompleted: open()
    onAboutToShow: now = Date.now()
    Timer { interval: 1000; repeat: true; running: popup.opened; onTriggered: popup.now = Date.now() }
    background: PanelSurface {}
    contentItem: ColumnLayout {
        spacing: Theme.spaceLg
        RowLayout {
            Layout.fillWidth: true
            Label {
                text: qsTranslate("Viewer", "Program details")
                color: Theme.textSecondary; font.pixelSize: Theme.fontCaption
                Layout.fillWidth: true
            }
            IconAction {
                objectName: "closeGuideProgramDetails"
                Layout.preferredWidth: Theme.compactControlHeight
                Layout.preferredHeight: Theme.compactControlHeight
                iconSource: popup.iconDirectory + "x.svg"
                tip: qsTranslate("Main", "Close")
                iconSize: Theme.smallIconSize
                flat: true
                onClicked: popup.close()
            }
        }
        ScrollView {
            id: detailsScroll
            Layout.fillWidth: true
            Layout.fillHeight: true
            rightPadding: Theme.spaceMd
            clip: true
            focus: true
            ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
            ScrollBar.vertical.policy: detailsFlick.contentHeight > detailsFlick.height
                ? ScrollBar.AlwaysOn : ScrollBar.AlwaysOff
            ScrollBar.vertical.palette.mid: Theme.textMuted
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
                    spacing: Theme.spaceLg
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
                        color: Theme.textSecondary; font.pixelSize: Theme.fontCaption
                        wrapMode: Text.Wrap
                    }
                    Label {
                        objectName: "programTitle"
                        width: parent.width
                        text: popup.program ? (popup.program.name || qsTranslate("Viewer", "No program information")) : ""
                        textFormat: Text.PlainText
                        color: Theme.textPrimary; font.pixelSize: Theme.fontTitle; font.bold: true
                        wrapMode: Text.Wrap; lineHeight: 1.15
                    }
                    Label {
                        objectName: "programChannel"
                        width: parent.width
                        text: popup.channelLabel; textFormat: Text.PlainText
                        color: Theme.textSecondary; wrapMode: Text.Wrap
                    }
                    ProgramFacts {
                        objectName: "programFacts"
                        width: parent.width
                        program: popup.program
                        now: popup.now
                    }
                    Rectangle { width: parent.width; height: 1; color: Theme.overlayBorder }
                    Label {
                        objectName: "programDescription"
                        width: parent.width
                        text: popup.program ? (popup.program.description || "") : ""
                        textFormat: Text.PlainText; color: Theme.textPrimary
                        visible: text.length > 0
                        wrapMode: Text.Wrap; font.pixelSize: Theme.fontBody; lineHeight: 1.25
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
                        wrapMode: Text.Wrap; color: Theme.error
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
        ActionButton {
            emphasis: ActionButton.Primary
            id: watchButton
            objectName: "watchGuideProgram"
            visible: popup.live
            Layout.preferredWidth: 168
            Layout.preferredHeight: 44
            text: qsTranslate("Viewer", "Watch this program")
            onClicked: popup.watchRequested(popup.program.watchKey)
        }
    }
}
