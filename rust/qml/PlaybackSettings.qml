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
    required property bool statsVisible
    signal danmakuRequested(bool enabled, real textSize, real textOpacity, real speed)
    signal statsRequested(bool visible)
    function toggle() {
        if (visible) close();
        else open();
    }
    parent: toggleButton
    width: 320
    height: 340
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
    contentItem: ColumnLayout {
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
