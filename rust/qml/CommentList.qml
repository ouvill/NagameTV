pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls

ListView {
    id: root
    required property string commentsJson
    required property string status
    clip: true
    model: ListModel { id: rows }
    boundsBehavior: Flickable.StopAtBounds
    ScrollBar.vertical: ScrollBar {
        objectName: "commentScrollBar"
        policy: ScrollBar.AlwaysOn
        onPressedChanged: if (pressed) root.cancelFollow()
    }
    property bool ready: false
    property bool followPending: false
    property int updateRevision: 0
    function cancelFollow() {
        followPending = false;
        ++updateRevision;
    }
    onMovementStarted: cancelFollow()
    Component.onCompleted: {
        ready = true;
        updateComments();
    }
    onCommentsJsonChanged: if (ready) updateComments()

    function updateComments() {
        const incoming = JSON.parse(commentsJson);
        const follow = rows.count === 0 || atYEnd || followPending;
        const revision = ++updateRevision;
        followPending = follow;
        const topIndex = indexAt(1, contentY + 1);
        const topItem = topIndex >= 0 ? itemAtIndex(topIndex) : null;
        const anchorId = topItem ? rows.get(topIndex).commentId : "";
        const offset = topItem ? contentY - topItem.y : 0;

        // Retain existing rows and their delegates, including duplicate comment text.
        let removed = 0;
        while (removed < rows.count && (incoming.length === 0 || rows.get(removed).commentId !== incoming[0].id))
            ++removed;
        if (removed > 0)
            rows.remove(0, removed);
        for (let i = rows.count; i < incoming.length; ++i) {
            const comment = incoming[i];
            rows.append({commentId: comment.id, time: comment.time, text: comment.text, source: comment.source});
        }
        forceLayout();
        if (follow) {
            positionViewAtEnd();
            // Wrapped delegates finish layout after the model mutation.
            Qt.callLater(function() {
                if (root.updateRevision === revision && root.followPending) {
                    root.forceLayout();
                    root.positionViewAtEnd();
                    root.followPending = false;
                }
            });
        } else if (removed > 0) {
            // The 200-row history can evict rows above the viewport. Restore the
            // same visible comment and pixel offset, or the oldest retained row.
            let anchor = -1;
            for (let i = 0; i < rows.count; ++i) {
                if (rows.get(i).commentId === anchorId) {
                    anchor = i;
                    break;
                }
            }
            if (anchor >= 0) {
                positionViewAtIndex(anchor, ListView.Beginning);
                forceLayout();
                const item = itemAtIndex(anchor);
                if (item)
                    contentY = item.y + offset;
            } else {
                positionViewAtBeginning();
            }
        }
    }
    delegate: Item {
        id: row
        required property string time
        required property string text
        required property string source
        width: root.width - 12
        height: Math.max(62, body.implicitHeight + 28)
        Label {
            x: 0; y: 16; width: 62
            text: row.time
            color: "#929497"; font.pixelSize: 10
        }
        Label {
            id: body
            x: 70; y: 13; width: parent.width - 70
            text: row.text; textFormat: Text.PlainText
            color: "#e5e5e4"; font.pixelSize: 14; wrapMode: Text.Wrap
        }
        Label {
            anchors.right: parent.right; anchors.bottom: parent.bottom; anchors.bottomMargin: 5
            text: row.source; textFormat: Text.PlainText
            color: "#b6bab6"; font.pixelSize: 9
        }
        Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: "#12ffffff" }
    }
    Label {
        anchors.centerIn: parent
        visible: root.count === 0
        width: parent.width - 24
        horizontalAlignment: Text.AlignHCenter; wrapMode: Text.Wrap
        text: root.status; textFormat: Text.PlainText
        color: "#b6bab6"; font.pixelSize: 13
    }
}
