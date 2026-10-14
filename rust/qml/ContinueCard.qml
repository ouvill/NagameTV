pragma ComponentBehavior: Bound
import QtQuick
import MinimalViewer
import QtQuick.Controls
import QtQuick.Layouts

// A recording stopped part-way: what it is and how much is left.
ItemDelegate {
    id: card
    required property string recordingKey
    required property string programName
    required property string channelName
    required property real progress
    required property real remainingMs
    required property string thumbnail
    readonly property int remainingMinutes: Math.max(1, Math.ceil(remainingMs / 60000))
    property real feedbackScale: down ? Theme.cardPressScale : highlighted ? Theme.cardFocusScale : 1
    Behavior on feedbackScale {
        NumberAnimation { duration: card.down ? Theme.pressDuration : Theme.moveDuration; easing.type: Easing.OutCubic }
    }
    z: highlighted ? 1 : 0
    padding: Theme.spaceMd
    hoverEnabled: true
    Accessible.name: programName
    background: CardSurface {
        scale: card.feedbackScale
        selected: card.highlighted
        hovered: card.hovered
        pressed: card.down
        focused: card.highlighted
    }
    contentItem: ColumnLayout {
        scale: card.feedbackScale
        spacing: Theme.spaceSm
        RecordingThumbnail {
            Layout.fillWidth: true
            Layout.preferredHeight: width * 9 / 16
            source: card.thumbnail
            ProgressLine {
                anchors { left: parent.left; right: parent.right; bottom: parent.bottom }
                square: true
                progress: card.progress
            }
        }
        Label {
            objectName: "continueTitle"
            Layout.fillWidth: true
            Layout.fillHeight: true
            text: card.programName
            textFormat: Text.PlainText
            color: Theme.textPrimary
            font.pixelSize: Theme.fontBody
            font.bold: true
            wrapMode: Text.Wrap
            maximumLineCount: 2
            elide: Text.ElideRight
            verticalAlignment: Text.AlignTop
        }
        RowLayout {
            Layout.fillWidth: true
            spacing: Theme.spaceSm
            Label {
                Layout.fillWidth: true
                text: card.channelName
                textFormat: Text.PlainText
                color: Theme.textSecondary
                font.pixelSize: Theme.fontCaption
                elide: Text.ElideRight
            }
            Label {
                objectName: "continueRemaining"
                text: qsTranslate("Viewer", "%1 min left").arg(card.remainingMinutes)
                color: Theme.accent
                font.pixelSize: Theme.fontCaption
            }
        }
    }
}
