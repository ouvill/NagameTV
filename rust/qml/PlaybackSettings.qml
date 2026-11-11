pragma ComponentBehavior: Bound
import QtQuick
import MinimalViewer
import QtQuick.Controls
import QtQuick.Layouts

ScrollView {
    id: root
    required property bool commentsEnabled
    required property bool danmakuEnabled
    required property real textSize
    required property real textOpacity
    required property real speed
    required property bool shadowEnabled
    required property bool statsVisible
    property string displayMode: "scroll"
    property string placementMode: "sequential"
    property string densityMode: "normal"
    property bool evaluationCollision: false
    property bool appearanceExpanded: false
    // Audio and subtitles use the same player actions as the playback bar popups.
    property bool playing: false
    property real volumeLevel: 1
    property bool muted: false
    property string audioTracksJson: "[]"
    // Translated by Rust, which also logs the failure.
    property string audioError: ""
    property bool subtitlesAvailable: false
    property bool subtitlesShown: false
    readonly property var audioTracks: JSON.parse(audioTracksJson)
    signal volumeRequested(real fraction)
    signal muteRequested(bool muted)
    signal volumeSaveRequested
    signal audioTrackRequested(string trackId)
    signal subtitlesRequested(bool shown)
    signal densityRequested(string densityMode)
    signal presentationRequested(string displayMode, string placementMode)
    signal danmakuRequested(bool enabled)
    signal adjusted(real textSize, real textOpacity, real speed)
    signal statsRequested(bool visible)
    signal shadowRequested(bool enabled)
    signal boundaryReached(int key)
    function enter() { navigation.enter(); }
    function enterLast() { navigation.enterLast(); }
    clip: true
    padding: Theme.spaceSm
    contentWidth: availableWidth
    FormNavigation {
        id: navigation
        scrollView: root
        fields: [volume.navigationItem, muteToggle, audioTrack, subtitleToggle, danmakuToggle, ...adjustments.navigationItems, appearanceButton,
            ...presentation.navigationItems, shadowToggle, statsToggle]
        onBoundaryReached: function(key) { root.boundaryReached(key); }
    }

    AudioLabels { id: audioLabels }
    ColumnLayout {
        width: root.availableWidth
        spacing: Theme.spaceLg
        SettingsSlider {
            id: volume
            objectName: "playbackVolume"
            Layout.fillWidth: true
            text: qsTranslate("Main", "Volume")
            valueText: root.muted ? qsTranslate("Remote", "Muted") : Math.round(root.volumeLevel * 100) + "%"
            from: 0; to: 100; stepSize: 5
            value: Math.round(root.volumeLevel * 100)
            onMoved: function(value) { root.volumeRequested(value / 100); }
            // Save once per adjustment, not for every step of a held key.
            Connections {
                target: volume.navigationItem
                function onPressedChanged() { if (!volume.navigationItem.pressed) root.volumeSaveRequested(); }
                function onActiveFocusChanged() { if (!volume.navigationItem.activeFocus) root.volumeSaveRequested(); }
            }
        }
        SettingsToggle {
            id: muteToggle
            objectName: "playbackMuteToggle"
            Layout.fillWidth: true
            text: qsTranslate("Main", "Mute")
            checked: root.muted
            onToggled: root.muteRequested(checked)
        }
        RowLayout {
            Layout.fillWidth: true
            visible: root.audioTracks.length > 1
            spacing: Theme.spaceLg
            Label {
                Layout.fillWidth: true
                text: qsTranslate("Viewer", "Audio")
                color: Theme.textPrimary
                font.pixelSize: Theme.fontControl
                opacity: audioTrack.enabled ? 1 : Theme.disabledOpacity
            }
            SettingsChoice {
                id: audioTrack
                objectName: "playbackAudioTrack"
                visible: root.audioTracks.length > 1
                enabled: root.playing
                Layout.preferredWidth: Math.min(220, root.availableWidth * 0.6)
                Accessible.name: qsTranslate("Viewer", "Audio")
                model: root.audioTracks.map((track, index) => audioLabels.trackLabel(track, index, root.audioTracks))
                currentIndex: root.audioTracks.findIndex(track => track.selected === true)
                onActivated: function(index) {
                    root.audioTrackRequested(root.audioTracks[index].id);
                    currentIndex = Qt.binding(function() { return root.audioTracks.findIndex(track => track.selected === true); });
                }
            }
        }
        Label {
            objectName: "playbackAudioError"
            Layout.fillWidth: true
            visible: text.length > 0
            text: root.audioError
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
            color: Theme.warning
            font.pixelSize: Theme.fontBody
        }
        SettingsToggle {
            id: subtitleToggle
            objectName: "playbackSubtitleToggle"
            Layout.fillWidth: true
            enabled: root.subtitlesAvailable
            text: qsTranslate("Settings", "Show subtitles")
            checked: root.subtitlesAvailable && root.subtitlesShown
            onToggled: root.subtitlesRequested(checked)
        }
        Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.divider }
        SettingsToggle {
            id: danmakuToggle
            objectName: "playbackDanmakuToggle"
            Layout.fillWidth: true
            enabled: root.commentsEnabled
            text: qsTranslate("Main", "Danmaku comments")
            checked: root.danmakuEnabled
            onToggled: root.danmakuRequested(checked)
        }
        DanmakuAdjustments {
            id: adjustments
            Layout.fillWidth: true
            enabled: root.commentsEnabled
            textSize: root.textSize
            textOpacity: root.textOpacity
            speed: root.speed
            onAdjusted: function(size, opacity, speed) { root.adjusted(size, opacity, speed); }
        }
        DisclosureButton {
            id: appearanceButton
            objectName: "playbackAppearanceButton"
            Layout.fillWidth: true
            enabled: root.commentsEnabled
            checked: root.appearanceExpanded
            text: qsTranslate("Settings", "Comment appearance")
            onToggled: root.appearanceExpanded = checked
        }
        CommentPresentation {
            id: presentation
            Layout.fillWidth: true
            visible: root.appearanceExpanded
            enabled: root.commentsEnabled
            displayMode: root.displayMode
            placementMode: root.placementMode
            densityMode: root.densityMode
            evaluationCollision: root.evaluationCollision
            onDensitySelected: function(density) { root.densityRequested(density); }
            onSelected: function(display, placement) { root.presentationRequested(display, placement); }
        }
        SettingsToggle {
            id: shadowToggle
            objectName: "playbackShadowToggle"
            Layout.fillWidth: true
            enabled: root.commentsEnabled
            visible: root.appearanceExpanded
            text: qsTranslate("Settings", "Drop shadow")
            checked: root.shadowEnabled
            onToggled: root.shadowRequested(checked)
        }
        Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.divider }
        SettingsToggle {
            id: statsToggle
            objectName: "playbackStatsToggle"
            Layout.fillWidth: true
            text: qsTranslate("Main", "Stats for nerds")
            checked: root.statsVisible
            onToggled: root.statsRequested(checked)
        }
    }
}
