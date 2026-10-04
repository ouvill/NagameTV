pragma ComponentBehavior: Bound
import QtQuick

Item {
    id: root
    required property InputContext inputContext
    signal remoteKey(string domKey)
    signal dataButton()
    property bool available: false
    Shortcut {
        sequence: "D"
        context: Qt.WindowShortcut
        autoRepeat: false
        enabled: root.available && root.inputContext.receiverInputEnabled && root.inputContext.viewing
        onActivated: root.dataButton()
    }
    // Native shortcuts win even when a clicked player control retains focus.
    // Claim only the groups requested by the current BML document.
    Repeater {
        model: [
            {sequence: "Up", key: "ArrowUp"}, {sequence: "Down", key: "ArrowDown"},
            {sequence: "Left", key: "ArrowLeft"}, {sequence: "Right", key: "ArrowRight"},
            {sequence: "Return", key: "Enter"}, {sequence: "Enter", key: "Enter"},
            {sequence: "Space", key: "Enter"}, {sequence: "Back", key: "Backspace"},
            {sequence: "Backspace", key: "Backspace"}, {sequence: "X", key: "Backspace"},
            {sequence: "B", key: "b"}, {sequence: "R", key: "r"},
            {sequence: "G", key: "g"}, {sequence: "Y", key: "y"},
            {sequence: "0", key: "0"}, {sequence: "1", key: "1"},
            {sequence: "2", key: "2"}, {sequence: "3", key: "3"},
            {sequence: "4", key: "4"}, {sequence: "5", key: "5"},
            {sequence: "6", key: "6"}, {sequence: "7", key: "7"},
            {sequence: "8", key: "8"}, {sequence: "9", key: "9"}
        ]
        delegate: Item {
            id: binding
            required property var modelData
            Shortcut {
                sequence: binding.modelData.sequence
                context: Qt.WindowShortcut
                autoRepeat: binding.modelData.key.startsWith("Arrow")
                enabled: root.available && root.inputContext.bmlAccepts(binding.modelData.sequence)
                onActivated: root.remoteKey(binding.modelData.key)
            }
        }
    }
}
