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
    onOpened: subtitleToggle.forceActiveFocus(Qt.TabFocusReason)
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
        Keys.onBackPressed: function(event) { if (!event.isAutoRepeat) popup.close(); }
        spacing: Theme.spaceMd
        Label { text: qsTranslate("Viewer", "Subtitles"); color: Theme.textPrimary; font.pixelSize: Theme.fontHeading }
        SettingsToggle {
            id: subtitleToggle
            objectName: "mediaSubtitleToggle"
            KeyNavigation.down: trackOptions.count > 0 ? trackOptions.itemAt(0) : openFileButton
            Layout.fillWidth: true
            text: qsTranslate("Main", "Show subtitles")
            checked: popup.backend.subtitle_display
            onToggled: popup.backend.display_subtitles(checked)
        }
        ScrollView {
            id: trackScroll
            Layout.fillWidth: true
            Layout.preferredHeight: Math.min(tracks.implicitHeight + topPadding + bottomPadding, 180)
            padding: Theme.spaceSm
            clip: true
            visible: trackOptions.count > 0
            Column {
                id: tracks
                width: trackScroll.availableWidth
                spacing: Theme.spaceSm
                Repeater {
                    id: trackOptions
                    model: popup.backend.subtitle_tracks
                    delegate: ActionButton {
                        required property string trackId
                        required property int index
                        required property bool trackSelected
                        selected: trackSelected
                        KeyNavigation.up: index > 0 ? trackOptions.itemAt(index - 1) : subtitleToggle
                        KeyNavigation.down: index + 1 < trackOptions.count ? trackOptions.itemAt(index + 1) : openFileButton
                        onActiveFocusChanged: {
                            if (!activeFocus) return;
                            const viewport = trackScroll.contentItem as Flickable;
                            if (viewport) viewport.contentY = Math.max(0, Math.min(y, viewport.contentHeight - viewport.height));
                        }
                        required property string displayName
                        width: tracks.width
                        text: displayName || qsTranslate("Viewer", "Subtitle %1").arg(index + 1)
                        onClicked: popup.backend.select_subtitle(trackId)
                    }
                }
            }
        }
        ActionButton {
            id: openFileButton
            objectName: "openSubtitleButton"
            KeyNavigation.up: trackOptions.count > 0 ? trackOptions.itemAt(trackOptions.count - 1) : subtitleToggle
            Layout.fillWidth: true
            text: qsTranslate("Viewer", "Open subtitle file…")
            enabled: !popup.backend.subtitle_loading
            onClicked: fileDialog.open()
        }
        Label {
            objectName: "mediaSubtitleStatus"
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
