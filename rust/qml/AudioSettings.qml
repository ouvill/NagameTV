pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Popup {
    id: popup
    enum Dismissal { Idle, Closing }
    QtObject {
        id: dismissal
        property int phase: AudioSettings.Idle
        property Item focusTarget: null
    }
    function toggle() {
        if (!visible || dismissal.phase === AudioSettings.Closing) open();
        else close();
    }
    function containsFocusItem(item) {
        for (let current = item; current; current = current.parent) {
            if (current === contentItem || current === contentItem.parent) return true;
        }
        return false;
    }
    Connections {
        target: popup.contentItem.Window.window
        function onActiveFocusItemChanged() {
            const item = target.activeFocusItem;
            // Qt restores the pre-popup focus at the end of an exit transition.
            // Preserve a new outside target chosen while that transition runs.
            if (dismissal.phase === AudioSettings.Closing && popup.visible
                    && item && item !== popup.anchorItem && !popup.containsFocusItem(item))
                dismissal.focusTarget = item;
        }
    }
    required property real windowWidth
    required property real windowHeight
    property Item anchorItem: null
    property bool shuttingDown: false
    property bool playing: false
    property real volumeLevel: 1
    property bool muted: false
    signal volumeRequested(real fraction)
    signal muteRequested(bool muted)
    signal saveRequested
    readonly property int touchTarget: 44
    readonly property int motionDuration: 140
    property string tracksJson: "[]"
    // Backend error translation source, not diagnostic text.
    property string errorText: ""
    property url iconDirectory: "qrc:/qt/qml/MinimalViewer/assets/icons/"
    readonly property var tracks: JSON.parse(tracksJson)
    AudioLabels { id: audioLabels }
    function trackLabel(track, index) {
        const language = audioLabels.language(track.language);
        const number = track.number || index + 1;
        const role = track.role === "main" ? qsTranslate("Main", "Main audio") : track.role === "sub" ? qsTranslate("Main", "Sub audio") : track.role === "both" ? qsTranslate("Main", "Main / sub") : "";
        const name = track.role === "both" ? role : (language ? language + (role ? " · " + role : "") : role);
        const duplicate = tracks.some(other => other.id !== track.id && other.language === track.language && other.role === track.role);
        const audioName = qsTranslate("Main", "Audio %1").arg(number);
        return !name ? audioName : duplicate ? name + " · " + audioName : name;
    }
    signal refreshRequested
    signal selectRequested(string trackId)
    parent: anchorItem ? anchorItem : Overlay.overlay
    width: Math.min(380, windowWidth - 40)
    height: Math.min(contentItem.implicitHeight + topPadding + bottomPadding, windowHeight - 80)
    x: anchorItem ? 0 : 24
    y: anchorItem ? -height - 12 : Math.max(20, windowHeight - height - 100)
    padding: 20
    modal: false
    dim: false
    focus: true
    closePolicy: Popup.CloseOnEscape | (anchorItem ? Popup.CloseOnPressOutsideParent : Popup.CloseOnPressOutside)
    onAboutToShow: {
        dismissal.phase = AudioSettings.Idle;
        dismissal.focusTarget = null;
        errorText = "";
        refreshRequested();
    }
    onAboutToHide: {
        volumeSlider.finishGesture();
        dismissal.phase = AudioSettings.Closing;
    }
    onClosed: {
        const target = dismissal.focusTarget;
        dismissal.phase = AudioSettings.Idle;
        dismissal.focusTarget = null;
        tracksJson = "[]";
        if (target && target.enabled && target.visible) target.forceActiveFocus(Qt.OtherFocusReason);
    }
    opacity: 0
    enter: Transition {
        NumberAnimation { property: "opacity"; to: 1; duration: popup.motionDuration; easing.type: Easing.OutCubic }
    }
    exit: Transition {
        NumberAnimation { property: "opacity"; to: 0; duration: popup.motionDuration; easing.type: Easing.OutCubic }
    }
    Timer {
        interval: 1000
        repeat: true
        running: popup.opened
        onTriggered: popup.refreshRequested()
    }
    background: Rectangle {
        radius: 18
        color: "#f21a1c1a"
        border.color: "#42ffffff"
    }
    contentItem: ColumnLayout {
        spacing: 12
        RowLayout {
            Layout.fillWidth: true
            Label {
                text: qsTranslate("Viewer", "Audio")
                color: "#f4f5f3"
                font.pixelSize: 17
                font.bold: true
                Layout.fillWidth: true
            }
            IconAction {
                iconSource: popup.iconDirectory + "x.svg"
                tip: qsTranslate("Main", "Close")
                implicitWidth: popup.touchTarget
                implicitHeight: popup.touchTarget
                onClicked: popup.close()
            }
        }
        RowLayout {
            Layout.fillWidth: true
            spacing: 12
            IconAction {
                objectName: "muteButton"
                implicitWidth: popup.touchTarget
                implicitHeight: popup.touchTarget
                active: popup.muted
                iconSource: popup.iconDirectory + (popup.muted || popup.volumeLevel === 0 ? "volume-x.svg" : "volume-2.svg")
                tip: popup.muted ? qsTranslate("Viewer", "Unmute") : qsTranslate("Viewer", "Mute")
                onClicked: popup.muteRequested(!popup.muted)
            }
            VolumeSlider {
                id: volumeSlider
                objectName: "playerVolumeSlider"
                Layout.fillWidth: true
                Layout.minimumHeight: popup.touchTarget
                value: popup.volumeLevel
                subdued: popup.muted
                closing: popup.shuttingDown
                onVolumeRequested: function(value) { popup.volumeRequested(value); }
                onSaveRequested: popup.saveRequested()
            }
            Label {
                Layout.preferredWidth: 44
                text: Math.round(popup.volumeLevel * 100) + "%"
                color: popup.muted ? "#b6bab6" : "#f4f5f3"
                font.pixelSize: 12
                horizontalAlignment: Text.AlignRight
                Accessible.ignored: true
            }
        }
        Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: "#343c35" }
        Label {
            text: qsTranslate("Main", "Audio selection")
            color: "#b6bab6"
            font.pixelSize: 12
        }
        ScrollView {
            id: scroll
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.preferredHeight: Math.min(180, options.implicitHeight)
            contentWidth: availableWidth
            clip: true
            ColumnLayout {
                id: options
                width: scroll.availableWidth
                spacing: 6
                Repeater {
                    model: popup.tracks
                    delegate: TextAction {
                        id: option
                        objectName: "audioOption" + index
                        required property int index
                        required property var modelData
                        Layout.fillWidth: true
                        Layout.minimumHeight: popup.touchTarget
                        Accessible.checkable: true
                        Accessible.checked: modelData.selected === true
                        enabled: popup.playing && modelData.enabled !== false && popup.tracks.length > 1
                        text: popup.trackLabel(modelData, index)
                        contentItem: Label {
                            text: option.text
                            textFormat: Text.PlainText
                            color: "#f4f5f3"
                            font.pixelSize: 12
                            wrapMode: Text.Wrap
                        }
                        background: Rectangle {
                            radius: 12
                            scale: option.feedbackScale
                            color: option.down ? "#589caf9f" : option.modelData.selected ? "#389caf9f" : option.hovered ? "#28302a" : "#1c1f1c"
                            Behavior on color { ColorAnimation { duration: 100 } }
                            border.color: option.modelData.selected || option.visualFocus ? "#9caf9f" : "#28ffffff"
                        }
                        onClicked: popup.selectRequested(modelData.id)
                    }
                }
                Label {
                    visible: popup.tracks.length === 0
                    Layout.fillWidth: true
                    text: qsTranslate("Main", "No audio tracks are available yet.")
                    color: "#b6bab6"
                    wrapMode: Text.Wrap
                    font.pixelSize: 12
                }
            }
        }
        Label {
            Layout.fillWidth: true
            visible: popup.tracks.length === 1
            text: qsTranslate("Main", "This broadcast has one audio option.")
            color: "#b6bab6"
            wrapMode: Text.Wrap
            font.pixelSize: 11
        }
        Label {
            Layout.fillWidth: true
            objectName: "audioError"
            visible: popup.errorText.length > 0
            text: popup.errorText.length ? qsTranslate("Backend", popup.errorText) : ""
            textFormat: Text.PlainText
            color: "#f4f5f3"
            wrapMode: Text.Wrap
            font.pixelSize: 12
        }
    }
}
