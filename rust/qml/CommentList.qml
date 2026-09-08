pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls

ListView {
    id: root
    property var commentModel: null
    required property string status
    clip: true
    model: commentModel
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
    function finishFollow(revision) {
        if (revision !== undefined && revision !== updateRevision)
            return;
        if (!ready || !followPending || width <= 0 || height <= 0)
            return;
        forceLayout();
        positionViewAtEnd();
        followPending = false;
    }
    onWidthChanged: if (followPending) Qt.callLater(finishFollow)
    onHeightChanged: if (followPending) Qt.callLater(finishFollow)
    onMovementStarted: cancelFollow()
    property string anchorId: ""
    property string firstId: ""
    property real anchorOffset: 0
    Component.onCompleted: {
        ready = true;
        resetFollow();
    }
    onModelChanged: if (ready) resetFollow()
    Connections {
        target: root.commentModel
        function onAbout_to_update() { root.prepareUpdate(); }
        function onUpdated() { root.finishUpdate(); }
    }
    function resetFollow() {
        ++updateRevision;
        followPending = true;
        if (width > 0 && height > 0) {
            forceLayout();
            positionViewAtEnd();
        }
        Qt.callLater(finishFollow, updateRevision);
    }
    function prepareUpdate() {
        if (!ready)
            return;
        followPending = count === 0 || atYEnd || followPending;
        ++updateRevision;
        const topIndex = indexAt(1, contentY + 1);
        const topItem = topIndex >= 0 ? itemAtIndex(topIndex) : null;
        anchorId = topItem ? commentModel.id_at(topIndex) : "";
        anchorOffset = topItem ? contentY - topItem.y : 0;
        firstId = commentModel.id_at(0);
    }
    function finishUpdate() {
        if (!ready)
            return;
        forceLayout();
        if (followPending) {
            positionViewAtEnd();
            // Wrapped delegates finish layout after the model mutation.
            // Pass the QML method directly so destruction cancels the callback.
            // Keep following pending if the Loader has not assigned our size yet.
            Qt.callLater(finishFollow, updateRevision);
        } else if (firstId !== commentModel.id_at(0)) {
            // The bounded history can evict rows above the viewport. Restore the
            // same visible comment and pixel offset, or the oldest retained row.
            const anchor = commentModel.row_for_id(anchorId);
            if (anchor >= 0) {
                positionViewAtIndex(anchor, ListView.Beginning);
                forceLayout();
                const item = itemAtIndex(anchor);
                if (item)
                    contentY = item.y + anchorOffset;
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
