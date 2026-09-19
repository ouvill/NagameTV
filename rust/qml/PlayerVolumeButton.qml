pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    id: root
    required property ViewerActions actions
    property url iconDirectory: "qrc:/qt/qml/MinimalViewer/assets/icons/"
    readonly property bool pressed: slider.pressed
    implicitWidth: 42
    implicitHeight: 42
    onEnabledChanged: if (!enabled) volume.close()

    IconAction {
        id: mute
        objectName: "muteButton"
        anchors.fill: parent
        flat: true
        iconSource: root.iconDirectory + (root.actions.backend.audio_muted || root.actions.backend.volume_level === 0 ? "volume-x.svg" : "volume-2.svg")
        tip: action.text
        toolTipEnabled: !volume.visible
        action: root.actions.toggleMute
        onHoveredChanged: {
            if (hovered) { dismiss.stop(); volume.open(); }
            else dismiss.restart();
        }
        Keys.onUpPressed: { volume.open(); slider.forceActiveFocus(Qt.TabFocusReason); }
        Keys.onDownPressed: { volume.open(); slider.forceActiveFocus(Qt.TabFocusReason); }
    }
    Timer {
        id: dismiss
        interval: 250
        onTriggered: if (!mute.hovered && !popupHover.hovered && !slider.pressed && !volume.activeFocus) volume.close()
    }
    Popup {
        id: volume
        objectName: "playerVolumePopup"
        parent: root
        y: -height - 8
        padding: 12
        margins: 12
        modal: false
        dim: false
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutsideParent
        onClosed: if (slider.activeFocus || audio.activeFocus) mute.forceActiveFocus(Qt.TabFocusReason)
        background: Rectangle { radius: 12; color: "#f21a1c1a"; border.color: "#343c35" }
        contentItem: RowLayout {
            spacing: 12
            HoverHandler {
                id: popupHover
                onHoveredChanged: if (hovered) dismiss.stop(); else dismiss.restart()
            }
            VolumeSlider {
                id: slider
                objectName: "playerVolumeSlider"
                Layout.preferredWidth: 132
                value: root.actions.backend.volume_level
                subdued: root.actions.backend.audio_muted
                closing: !root.actions.enabled
                onVolumeRequested: function(fraction) { root.actions.backend.volume(fraction); }
                onSaveRequested: root.actions.backend.save_settings()
            }
            IconAction {
                id: audio
                objectName: "audioSelectionButton"
                flat: true
                iconSource: root.iconDirectory + "chevron-down.svg"
                tip: action.text
                action: root.actions.openAudio
                onClicked: volume.close()
            }
        }
    }
}
