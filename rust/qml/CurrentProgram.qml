pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls

Column {
    id: root
    property string programStatus: "unavailable"
    required property string programJson
    property bool recording: false
    property string fallbackTitle: ""
    property string channelLabel: ""
    property string logoUrl: ""
    readonly property var program: JSON.parse(programJson)
    signal detailsRequested
    spacing: 8
    Row {
        spacing: 12
        ChannelLogo {
            width: 64
            height: 36
            logoUrl: root.logoUrl
        }
        Label {
            anchors.verticalCenter: parent.verticalCenter
            text: root.channelLabel ? root.channelLabel.replace(/^\d+\s+/, "") : qsTranslate("Main", "Channels")
            color: "#b6bab6"
            font.pixelSize: 13
            textFormat: Text.PlainText
            style: Text.Outline
            styleColor: "#90000000"
        }
    }
    AbstractButton {
        id: currentButton
        objectName: "currentProgramButton"
        width: parent.width
        implicitHeight: contentItem.implicitHeight
        text: root.program ? (root.program.name || root.fallbackTitle || qsTranslate("Viewer", "Program title unavailable")) : root.programStatus === "pending" ? qsTranslate("Viewer", "Acquiring program information…") : root.programStatus === "failed" ? qsTranslate("Viewer", "Could not read program information") : qsTranslate("Main", "No program information")
        enabled: root.program !== null
        onClicked: root.detailsRequested()
        background: Rectangle {
            color: "transparent"
            border.width: currentButton.visualFocus ? 1 : 0
            border.color: "#9caf9f"
        }
        contentItem: Label {
            text: currentButton.text
            textFormat: Text.PlainText
            color: "#f4f5f3"
            font.pixelSize: 23
            font.bold: true
            wrapMode: Text.Wrap
            maximumLineCount: 2
            elide: Text.ElideRight
            style: Text.Outline
            styleColor: "#a0000000"
        }
    }
    Label {
        text: root.program && root.program.startAt !== null && root.program.duration !== null ? Qt.formatDateTime(new Date(root.program.startAt), root.recording ? "yyyy/MM/dd hh:mm" : "hh:mm") + " – " + Qt.formatDateTime(new Date(root.program.startAt + root.program.duration), "hh:mm") : ""
        color: "#d7d7d6"
        font.pixelSize: 12
        style: Text.Outline
        styleColor: "#90000000"
    }
}
