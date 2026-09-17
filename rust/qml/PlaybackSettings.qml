import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Popup {
    id: popup
    required property Item toggleButton
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
    signal danmakuRequested(bool enabled, real textSize, real textOpacity, real speed)
    property string timeshiftStorage: "memory"
    signal timeshiftRequested(string storage)
    signal timeshiftSettingsRequested()
    signal statsRequested(bool visible)
    signal shadowRequested(bool enabled)
    function toggle() {
        if (visible) close();
        else open();
    }
    parent: toggleButton
    width: 320
    readonly property real openerBottomMargin: Overlay.overlay
        ? Overlay.overlay.height - toggleButton.mapToItem(Overlay.overlay, 0, 0).y : 64
    height: Math.min(600, (Overlay.overlay ? Overlay.overlay.height : 640) - openerBottomMargin - 28 - margins)
    x: (toggleButton.width - width) / 2
    y: -height - 28
    margins: 20
    modal: false
    dim: false
    padding: 20
    focus: true
    // Leave the opener's click to toggle(); outside dismissal must not reopen it.
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutsideParent
    background: Rectangle {
        radius: 18
        color: "#f21a1c1a"
        border.color: "#42ffffff"
    }
    contentItem: ScrollView {
        id: scroll
        clip: true
        contentWidth: availableWidth
        ColumnLayout {
            width: scroll.availableWidth
            spacing: 8
            Label {
                text: qsTranslate("Main", "Playback settings")
                color: "#f4f5f3"
                font.pixelSize: 17
                font.bold: true
            }
            RowLayout {
                Layout.fillWidth: true
                enabled: popup.commentsEnabled
                Label { text: qsTranslate("Main", "Danmaku comments"); color: "#f4f5f3"; font.pixelSize: 13 }
                Item { Layout.fillWidth: true }
                ToggleSwitch {
                    objectName: "playbackDanmakuToggle"
                    text: qsTranslate("Main", "Danmaku comments")
                    checked: popup.danmakuEnabled
                    onToggled: popup.danmakuRequested(checked, popup.textSize, popup.textOpacity, popup.speed)
                }
            }
            CommentPresentation {
                Layout.fillWidth: true
                enabled: popup.commentsEnabled
                displayMode: popup.displayMode
                placementMode: popup.placementMode
                evaluationCollision: popup.evaluationCollision
                onSelected: function(display, placement) { popup.presentationRequested(display, placement); }
            }
            DanmakuAdjustments {
                Layout.fillWidth: true
                Layout.fillHeight: true
                spacing: 8
                enabled: popup.commentsEnabled
                textSize: popup.textSize
                textOpacity: popup.textOpacity
                speed: popup.speed
                onAdjusted: function(size, opacity, speed) {
                    popup.danmakuRequested(popup.danmakuEnabled, size, opacity, speed);
                }
            }
            RowLayout {
                Layout.fillWidth: true
                enabled: popup.commentsEnabled
                Label { text: qsTranslate("Settings", "Drop shadow"); color: "#f4f5f3"; font.pixelSize: 13 }
                Item { Layout.fillWidth: true }
                ToggleSwitch {
                    objectName: "playbackShadowToggle"
                    text: qsTranslate("Settings", "Drop shadow")
                    checked: popup.shadowEnabled
                    onToggled: popup.shadowRequested(checked)
                }
            }
            Button {
                Layout.fillWidth: true
                text: qsTranslate("Viewer", "Timeshift settings…")
                onClicked: { popup.close(); popup.timeshiftSettingsRequested(); }
            }
            RowLayout {
                Layout.fillWidth: true
                Label { text: qsTranslate("Main", "Stats for nerds"); color: "#f4f5f3"; font.pixelSize: 13 }
                Item { Layout.fillWidth: true }
                ToggleSwitch {
                    objectName: "playbackStatsToggle"
                    text: qsTranslate("Main", "Stats for nerds")
                    checked: popup.statsVisible
                    onToggled: popup.statsRequested(checked)
                }
            }
        }
    }
}
