pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import MinimalViewer

Popup {
    id: popup
    required property Player backend
    required property Item anchorItem
    required property real windowWidth
    required property real windowHeight
    parent: anchorItem
    width: Math.min(380, windowWidth - 40)
    height: Math.min(contentItem.implicitHeight + topPadding + bottomPadding, windowHeight - 80)
    x: anchorItem.width - width
    y: -height - 12
    padding: Theme.spaceXl
    focus: true
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutsideParent
    background: PanelSurface {}
    enter: Transition { NumberAnimation { property: "opacity"; from: 0; to: 1; duration: Theme.fadeInDuration } }
    exit: Transition { NumberAnimation { property: "opacity"; from: 1; to: 0; duration: Theme.fadeOutDuration } }
    Connections {
        target: popup.backend
        function onMedia_subtitle_availableChanged() { if (!popup.backend.media_subtitle_available) popup.close(); }
    }
    FileDialog {
        id: fileDialog
        title: qsTranslate("Viewer", "Open subtitle file")
        fileMode: FileDialog.OpenFile
        nameFilters: [qsTranslate("Viewer", "Subtitles (*.srt *.ass *.ssa)")]
        onAccepted: popup.backend.open_subtitle(selectedFile)
    }
    contentItem: ColumnLayout {
        spacing: Theme.spaceMd
        Label { text: qsTranslate("Viewer", "Subtitles"); color: Theme.textPrimary; font.pixelSize: Theme.fontHeading }
        SettingsToggle {
            Layout.fillWidth: true
            text: qsTranslate("Viewer", "Show subtitles")
            checked: popup.backend.subtitle_display
            onToggled: popup.backend.display_subtitles(checked)
        }
        ScrollView {
            Layout.fillWidth: true
            Layout.preferredHeight: Math.min(tracks.implicitHeight, 180)
            clip: true
            Column {
                id: tracks
                width: parent.width
                spacing: Theme.spaceSm
                Repeater {
                    model: popup.backend.subtitle_tracks
                    delegate: ActionButton {
                        required property string trackId
                        required property int index
                        required property bool trackSelected
                        selected: trackSelected
                        required property string displayName
                        width: tracks.width
                        text: displayName || qsTranslate("Viewer", "Subtitle %1").arg(index + 1)
                        onClicked: popup.backend.select_subtitle(trackId)
                    }
                }
            }
        }
        ActionButton {
            objectName: "openSubtitleButton"
            Layout.fillWidth: true
            text: qsTranslate("Viewer", "Open subtitle file…")
            enabled: !popup.backend.subtitle_loading
            onClicked: fileDialog.open()
        }
        Label {
            Layout.fillWidth: true
            visible: popup.backend.subtitle_loading || text.length > 0
            text: popup.backend.subtitle_loading ? qsTranslate("Viewer", "Loading subtitles…") : popup.backend.media_subtitle_error
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
            color: Theme.textSecondary
            font.pixelSize: Theme.fontCaption
        }
    }
}
