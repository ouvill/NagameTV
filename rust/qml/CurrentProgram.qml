pragma ComponentBehavior: Bound
import QtQuick
import MinimalViewer
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
    // 0..1 through the current live program; negative when unknown.
    property real progress: -1
    property bool favorite: false
    // Only a live channel from the loaded history can be starred.
    property bool favoriteEnabled: false
    readonly property int remainingMinutes: !recording && program && program.duration > 0 && !program.endUnknown && progress >= 0
        ? Math.ceil(program.duration * (1 - Math.min(1, progress)) / 60000) : 0
    signal detailsRequested
    signal favoriteRequested
    spacing: Theme.spaceSm
    Row {
        spacing: Theme.spaceMd
        ChannelLogo {
            width: 72
            height: 40
            // A recording or file has no station logo to wait for.
            visible: root.logoUrl.length > 0
            logoUrl: root.logoUrl
        }
        Label {
            anchors.verticalCenter: parent.verticalCenter
            text: root.channelLabel ? root.channelLabel.replace(/^\d+\s+/, "") : qsTranslate("Main", "Channels")
            color: Theme.textPrimary
            font.pixelSize: Theme.fontBody
            textFormat: Text.PlainText
            style: Text.Outline
            styleColor: Theme.textShadow
        }
        FavoriteToggle {
            objectName: "currentFavorite"
            anchors.verticalCenter: parent.verticalCenter
            visible: root.favoriteEnabled
            favorite: root.favorite
            onFavoriteToggled: root.favoriteRequested()
        }
    }
    AbstractButton {
        id: currentButton
        objectName: "currentProgramButton"
        width: parent.width
        implicitHeight: contentItem.implicitHeight
        text: root.program && root.program.scheduleState === "conflict" ? qsTranslate("Viewer", "Conflicting schedules") : root.program ? (root.program.name || root.fallbackTitle || qsTranslate("Viewer", "Program title unavailable")) : root.programStatus === "pending" ? qsTranslate("Settings", "Acquiring program information…") : root.programStatus === "failed" ? qsTranslate("Settings", "Could not read program information") : qsTranslate("Main", "No program information")
        enabled: root.program !== null
        onClicked: root.detailsRequested()
        background: Rectangle {
            color: "transparent"
            border.width: currentButton.visualFocus ? 1 : 0
            border.color: Theme.accent
        }
        contentItem: Label {
            text: currentButton.text
            textFormat: Text.PlainText
            color: Theme.textPrimary
            font.pixelSize: Theme.fontDisplay
            font.bold: true
            wrapMode: Text.Wrap
            maximumLineCount: 2
            elide: Text.ElideRight
            style: Text.Outline
            styleColor: Theme.textShadow
        }
    }
    Row {
        spacing: Theme.spaceMd
        Label {
            objectName: "currentProgramTime"
            text: root.program && root.program.endUnknown ? qsTranslate("Viewer", "End time unknown") : root.program && root.program.startAt !== null && root.program.duration !== null ? Qt.formatDateTime(new Date(root.program.startAt), root.recording ? "yyyy/MM/dd hh:mm" : "hh:mm") + " – " + Qt.formatDateTime(new Date(root.program.startAt + root.program.duration), "hh:mm") : ""
            color: Theme.textSecondary
            font.pixelSize: Theme.fontBody
            style: Text.Outline
            styleColor: Theme.textShadow
        }
        Label {
            objectName: "currentProgramRemaining"
            visible: root.remainingMinutes > 0
            text: qsTranslate("Viewer", "%1 min left").arg(root.remainingMinutes)
            // Over bright video the accent loses its outline contrast.
            color: Theme.textPrimary
            font.pixelSize: Theme.fontBody
            style: Text.Outline
            styleColor: Theme.textShadow
        }
    }
}
