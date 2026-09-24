import QtQuick
import MinimalViewer
import QtQuick.Controls
import QtQuick.Layouts

ColumnLayout {
    id: root
    required property var program
    property bool emphasized: false
    spacing: 5
    required property real now
    readonly property real progress: program && program.duration > 0 ? Math.max(0, Math.min(1, (now - program.startAt) / program.duration)) : 0
    Label {
        objectName: "cardProgramTitle"
        Layout.fillWidth: true
        text: root.program && root.program.scheduleState === "conflict" ? qsTranslate("Viewer", "Conflicting schedules") : root.program ? (root.program.name || qsTranslate("Viewer", "Program title unavailable")) : qsTranslate("Viewer", "No current program information")
        color: Theme.textPrimary
        font.pixelSize: root.emphasized ? Theme.fontControl : Theme.fontBody
        font.bold: true
        textFormat: Text.PlainText
        wrapMode: Text.Wrap
        maximumLineCount: 2
        elide: Text.ElideRight
    }
    Label {
        Layout.fillWidth: true
        text: root.program && root.program.scheduleState === "unknownEnd" ? qsTranslate("Viewer", "End time unknown") : root.program && !root.program.scheduleState ? Qt.formatDateTime(new Date(root.program.startAt), "hh:mm") + " – " + Qt.formatDateTime(new Date(root.program.startAt + root.program.duration), "hh:mm") : ""
        color: Theme.textSecondary
        font.pixelSize: Theme.fontCaption
    }
    Label {
        objectName: "cardNextProgram"
        readonly property var nextProgram: root.program ? root.program.next : null
        Layout.fillWidth: true
        visible: !!nextProgram
        text: nextProgram ? qsTranslate("Viewer", "Next %1 %2")
            .arg(Qt.formatDateTime(new Date(nextProgram.startAt), "hh:mm"))
            .arg(nextProgram.name || qsTranslate("Viewer", "Program title unavailable")) : ""
        color: Theme.textSecondary
        font.pixelSize: Theme.fontCaption
        textFormat: Text.PlainText
        elide: Text.ElideRight
    }
    Item {
        Layout.fillHeight: true
    }
    ProgressBar {
        Layout.fillWidth: true
        Layout.preferredHeight: 3
        background: Rectangle {
            color: Theme.overlayBorder
            radius: Theme.indicatorRadius
        }
        contentItem: Item {
            Rectangle {
                width: parent.width * root.progress
                height: parent.height
                radius: Theme.indicatorRadius
                color: Theme.accent
            }
        }
        from: 0
        to: 1
        value: root.progress
    }
}
