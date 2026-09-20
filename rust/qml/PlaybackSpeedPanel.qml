pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Popup {
    id: popup
    enum Gesture { Idle, Dragging, Cancelled }
    enum Dismissal { Ready, Closing }
    property int dismissal: PlaybackSpeedPanel.Ready
    property Item outsideFocusTarget: null
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
            if (popup.dismissal === PlaybackSpeedPanel.Closing && popup.visible
                    && item && item !== popup.anchorItem && !popup.containsFocusItem(item))
                popup.outsideFocusTarget = item;
        }
    }
    required property var backend
    required property Item anchorItem
    required property real windowWidth
    required property real windowHeight
    property int draft: backend.requested_playback_rate
    property string failure: ""
    readonly property int normalRate: 10
    readonly property int rateStep: 1
    readonly property var presets: [10, 12, 15, 20]
    readonly property int touchTarget: 42
    readonly property bool changing: backend.playback_rate !== backend.requested_playback_rate
    signal activity
    function label(tenths) { return "x" + (tenths / normalRate).toFixed(1); }
    function toggle() { if (!visible || dismissal === PlaybackSpeedPanel.Closing) open(); else close(); }
    function apply(tenths) {
        if (!backend.speed_available) return;
        const value = Math.max(backend.minimum_playback_rate, Math.min(backend.maximum_playback_rate, Math.round(tenths)));
        failure = "";
        if (!backend.set_playback_rate(value)) failure = backend.transport_error;
        draft = backend.requested_playback_rate;
        activity();
    }
    parent: anchorItem
    width: Math.min(320, windowWidth - 40)
    x: Math.max(12 - anchorItem.mapToItem(null, 0, 0).x,
        Math.min(0, windowWidth - anchorItem.mapToItem(null, 0, 0).x - width - 12))
    y: -height - 12
    padding: 18
    modal: false
    dim: false
    focus: true
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutsideParent
    onAboutToShow: { dismissal = PlaybackSpeedPanel.Ready; outsideFocusTarget = null; failure = ""; draft = backend.requested_playback_rate; activity(); }
    onAboutToHide: { dismissal = PlaybackSpeedPanel.Closing; speedSlider.cancelGesture(); activity(); }
    onClosed: {
        const target = outsideFocusTarget;
        outsideFocusTarget = null;
        if (target && target.enabled && target.visible) target.forceActiveFocus(Qt.OtherFocusReason);
    }
    Connections {
        target: popup.backend
        function onRequested_playback_rateChanged() {
            if (!speedSlider.pressed) popup.draft = popup.backend.requested_playback_rate;
        }
        function onSpeed_availableChanged() {
            if (!popup.backend.speed_available) speedSlider.cancelGesture();
        }
        function onTransport_errorChanged() {
            if (popup.visible) popup.failure = popup.backend.transport_error;
        }
        function onMedia_activeChanged() { if (!popup.backend.media_active) popup.close(); }
        function onRecording_nameChanged() { popup.close(); }
        function onSelectedChanged() { popup.close(); }
    }
    opacity: 0
    enter: Transition { NumberAnimation { property: "opacity"; to: 1; duration: 140; easing.type: Easing.OutCubic } }
    exit: Transition { NumberAnimation { property: "opacity"; to: 0; duration: 140; easing.type: Easing.OutCubic } }
    background: Rectangle { radius: 18; color: "#f21a1c1a"; border.color: "#42ffffff" }
    contentItem: ColumnLayout {
        spacing: 12
        RowLayout {
            Layout.fillWidth: true
            Label { text: qsTranslate("Viewer", "Playback speed"); color: "#f4f5f3"; font.pixelSize: 17; font.bold: true; Layout.fillWidth: true }
            Label { objectName: "speedValue"; text: popup.label(popup.draft); color: "#9caf9f"; font.pixelSize: 17 }
        }
        RowLayout {
            Layout.fillWidth: true
            spacing: 4
            TextAction {
                objectName: "speedDecrease"
                text: "−"
                implicitWidth: popup.touchTarget; implicitHeight: popup.touchTarget
                enabled: popup.backend.speed_available && popup.draft > popup.backend.minimum_playback_rate
                opacity: enabled ? 1 : 0.38
                Accessible.name: qsTranslate("Viewer", "Decrease playback speed")
                onClicked: popup.apply(popup.draft - popup.rateStep)
            }
            ThemedSlider {
                id: speedSlider
                objectName: "speedSlider"
                property int gesture: PlaybackSpeedPanel.Idle
                from: popup.backend.minimum_playback_rate
                to: popup.backend.maximum_playback_rate
                stepSize: popup.rateStep
                snapMode: Slider.SnapAlways
                value: popup.draft
                enabled: popup.backend.speed_available
                opacity: enabled ? 1 : 0.38
                Layout.fillWidth: true
                Layout.preferredHeight: popup.touchTarget
                Accessible.name: qsTranslate("Viewer", "Playback speed")
                function cancelGesture() {
                    gesture = PlaybackSpeedPanel.Cancelled;
                    popup.draft = popup.backend.requested_playback_rate;
                }
                onPressedChanged: {
                    if (pressed) gesture = PlaybackSpeedPanel.Dragging;
                    else {
                        if (gesture === PlaybackSpeedPanel.Dragging && popup.visible) popup.apply(value);
                        gesture = PlaybackSpeedPanel.Idle;
                    }
                }
                onMoved: {
                    popup.draft = Math.round(value);
                    if (!pressed) popup.apply(value);
                }
                Keys.onEscapePressed: function(event) { cancelGesture(); popup.close(); event.accepted = true; }
            }
            TextAction {
                objectName: "speedIncrease"
                text: "+"
                implicitWidth: popup.touchTarget; implicitHeight: popup.touchTarget
                enabled: popup.backend.speed_available && popup.draft < popup.backend.maximum_playback_rate
                opacity: enabled ? 1 : 0.38
                Accessible.name: qsTranslate("Viewer", "Increase playback speed")
                onClicked: popup.apply(popup.draft + popup.rateStep)
            }
        }
        RowLayout {
            Layout.fillWidth: true
            Label { text: popup.label(popup.backend.minimum_playback_rate); color: "#b6bab6"; font.pixelSize: 11; Layout.fillWidth: true }
            Label { text: popup.label(popup.backend.maximum_playback_rate); color: "#b6bab6"; font.pixelSize: 11 }
        }
        RowLayout {
            Layout.fillWidth: true
            spacing: 6
            Repeater {
                objectName: "speedPresets"
                model: popup.presets
                delegate: TextAction {
                    id: presetAction
                    required property int modelData
                    objectName: "speedPreset" + modelData
                    Layout.fillWidth: true
                    implicitHeight: popup.touchTarget
                    text: popup.label(modelData)
                    enabled: popup.backend.speed_available
                    opacity: enabled ? 1 : 0.38
                    Accessible.checkable: true
                    Accessible.checked: popup.backend.requested_playback_rate === modelData
                    background: Rectangle {
                        radius: 12
                        scale: presetAction.feedbackScale
                        color: presetAction.down ? "#589caf9f" : presetAction.Accessible.checked ? "#389caf9f" : presetAction.hovered ? "#28302a" : "#1c1f1c"
                        border.color: presetAction.Accessible.checked || presetAction.visualFocus ? "#9caf9f" : "#28ffffff"
                        Behavior on color { ColorAnimation { duration: 100 } }
                    }
                    onClicked: popup.apply(modelData)
                }
            }
        }
        TextAction {
            objectName: "speedReset"
            Layout.fillWidth: true
            implicitHeight: popup.touchTarget
            text: qsTranslate("Viewer", "Reset to normal speed")
            enabled: popup.backend.speed_available && popup.draft !== popup.normalRate
            opacity: enabled ? 1 : 0.38
            onClicked: popup.apply(popup.normalRate)
        }
        Label {
            objectName: "speedStatus"
            Layout.fillWidth: true
            visible: text.length > 0
            text: popup.failure || (!popup.backend.speed_available ? popup.backend.speed_reason
                : popup.changing ? qsTranslate("Viewer", "Changing to %1…").arg(popup.label(popup.backend.requested_playback_rate)) : "")
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
            color: "#b6bab6"
            font.pixelSize: 12
        }
    }
}
