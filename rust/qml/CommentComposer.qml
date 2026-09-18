pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls

Rectangle {
    id: root
    property string draft: ""
    property string status: ""
    property bool busy: false
    property bool available: false
    property bool supported: true
    required property CommentSubmitPolicy submitPolicy
    property url iconDirectory: "qrc:/qt/qml/MinimalViewer/assets/icons/"
    readonly property int maximumLength: 1024
    readonly property real occupiedHeight: height + (feedback.visible ? feedback.height + 8 : 0)
    readonly property string sendHint: submitPolicy.hint
    property bool showFeedback: false
    signal draftEdited(string text)
    signal sendRequested
    implicitWidth: 760
    implicitHeight: 46
    radius: 14
    color: "#d1171819"
    border.color: editor.activeFocus ? "#9caf9f" : "#7a69716d"
    onStatusChanged: showFeedback = status.length > 0

    function focusEditor() { editor.forceActiveFocus(); }
    function send() {
        if (available && !busy && editor.text.trim().length > 0 && !editor.inputMethodComposing)
            sendRequested();
    }

    TextField {
        id: editor
        objectName: "commentEditor"
        property bool normalizing: false
        anchors {
            left: parent.left; leftMargin: 24
            right: counter.left; rightMargin: 16
            top: parent.top; bottom: parent.bottom
        }
        padding: 0
        text: root.draft
        placeholderText: root.supported ? qsTranslate("Main", "Enter a comment…")
            : qsTranslate("Backend", "Comments are unavailable for this channel")
        Accessible.name: qsTranslate("Main", "Enter a comment…")
        Accessible.description: root.sendHint
        color: "#f4f5f3"
        placeholderTextColor: "#b8b6bab6"
        selectionColor: "#9caf9f"
        selectedTextColor: "#17201a"
        font.pixelSize: 18
        verticalAlignment: TextInput.AlignVCenter
        selectByMouse: true
        readOnly: root.busy
        clip: true
        background: null
        onTextChanged: {
            // Backend updates (including clearing an acknowledged draft) already
            // have the canonical text. Do not write back from a binding update.
            if (normalizing || text === root.draft) return;
            // Pasted line breaks become spaces; never split a surrogate pair at the limit.
            let normalized = text.replace(/[\r\n\u2028\u2029]+/g, " ");
            if (normalized.length > root.maximumLength) {
                let end = root.maximumLength;
                const last = normalized.charCodeAt(end - 1);
                if (last >= 0xd800 && last <= 0xdbff) --end;
                normalized = normalized.slice(0, end);
            }
            if (text !== normalized) {
                const cursor = cursorPosition;
                normalizing = true;
                remove(0, length);
                insert(0, normalized);
                cursorPosition = Math.min(cursor, length);
                normalizing = false;
            }
            if (text !== root.draft) {
                root.showFeedback = false;
                root.draftEdited(text);
            }
        }
        Keys.priority: Keys.BeforeItem
        Keys.onPressed: function(event) {
            const disposition = root.submitPolicy.disposition(event, inputMethodComposing);
            if (disposition === CommentSubmitPolicy.PassThrough) return;
            event.accepted = true;
            if (disposition === CommentSubmitPolicy.Submit) root.send();
        }
    }
    Label {
        id: counter
        objectName: "commentCharacterCount"
        anchors {
            right: sendButton.left; rightMargin: root.width >= 600 ? 72 : 12
            verticalCenter: parent.verticalCenter
        }
        width: 80
        text: editor.length + " / " + root.maximumLength
        color: "#b88c918c"
        font.pixelSize: 13
        horizontalAlignment: Text.AlignRight
    }
    IconAction {
        id: sendButton
        objectName: "sendComment"
        anchors { right: parent.right; rightMargin: 8; verticalCenter: parent.verticalCenter }
        implicitWidth: 46
        implicitHeight: 40
        iconSource: root.iconDirectory + "send.svg"
        tip: qsTranslate("Main", "Send") + " · " + root.sendHint
        enabled: root.available && !root.busy && editor.text.trim().length > 0 && !editor.inputMethodComposing
        opacity: enabled ? 1 : 0.4
        background: Rectangle {
            radius: 10
            color: sendButton.hovered ? "#28ffffff" : "transparent"
            border.color: sendButton.visualFocus ? "#9caf9f" : "transparent"
        }
        onClicked: { root.send(); root.focusEditor(); }
    }
    Label {
        id: feedback
        objectName: "commentPostHint"
        anchors { left: parent.left; right: parent.right; bottom: parent.top; bottomMargin: 8 }
        visible: root.showFeedback && root.status.length > 0
        text: root.status
        textFormat: Text.PlainText
        color: "#f4f5f3"
        font.pixelSize: 12
        wrapMode: Text.Wrap
        padding: 8
        background: Rectangle { radius: 8; color: "#e6171819" }
    }
}
