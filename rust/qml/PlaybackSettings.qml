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
    property bool evaluationCollision: false
    signal presentationRequested(string displayMode, string placementMode)
    signal danmakuRequested(bool enabled)
    signal adjusted(real textSize, real textOpacity, real speed)
    signal timeshiftSettingsRequested()
    signal statsRequested(bool visible)
    signal shadowRequested(bool enabled)
    clip: true
    contentWidth: availableWidth

    ColumnLayout {
        width: root.availableWidth
        spacing: Theme.spaceXl
        RowLayout {
            Layout.fillWidth: true
            enabled: root.commentsEnabled
            Label {
                text: qsTranslate("Main", "Danmaku comments")
                color: Theme.textPrimary; font.pixelSize: Theme.fontBody
                Layout.fillWidth: true
                wrapMode: Text.Wrap
            }
            ToggleSwitch {
                objectName: "playbackDanmakuToggle"
                text: qsTranslate("Main", "Danmaku comments")
                checked: root.danmakuEnabled
                onToggled: root.danmakuRequested(checked)
            }
        }
        CommentPresentation {
            Layout.fillWidth: true
            enabled: root.commentsEnabled
            displayMode: root.displayMode
            placementMode: root.placementMode
            evaluationCollision: root.evaluationCollision
            onSelected: function(display, placement) { root.presentationRequested(display, placement); }
        }
        DanmakuAdjustments {
            Layout.fillWidth: true
            enabled: root.commentsEnabled
            textSize: root.textSize
            textOpacity: root.textOpacity
            speed: root.speed
            onAdjusted: function(size, opacity, speed) { root.adjusted(size, opacity, speed); }
        }
        RowLayout {
            Layout.fillWidth: true
            enabled: root.commentsEnabled
            Label {
                text: qsTranslate("Settings", "Drop shadow")
                color: Theme.textPrimary; font.pixelSize: Theme.fontBody
                Layout.fillWidth: true; wrapMode: Text.Wrap
            }
            ToggleSwitch {
                objectName: "playbackShadowToggle"
                text: qsTranslate("Settings", "Drop shadow")
                checked: root.shadowEnabled
                onToggled: root.shadowRequested(checked)
            }
        }
        Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.divider }
        ActionButton {
            id: timeshift
            objectName: "playbackTimeshiftSettings"
            Layout.fillWidth: true
            text: qsTranslate("Viewer", "Timeshift settings…")
            onClicked: root.timeshiftSettingsRequested()
        }
        RowLayout {
            Layout.fillWidth: true
            Label {
                text: qsTranslate("Main", "Stats for nerds")
                color: Theme.textPrimary; font.pixelSize: Theme.fontBody
                Layout.fillWidth: true; wrapMode: Text.Wrap
            }
            ToggleSwitch {
                objectName: "playbackStatsToggle"
                text: qsTranslate("Main", "Stats for nerds")
                checked: root.statsVisible
                onToggled: root.statsRequested(checked)
            }
        }
    }
}
