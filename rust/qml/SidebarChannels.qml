pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls

Item {
    id: root
    required property var rows
    required property int selected
    property int viewingIndex: -1
    readonly property int openingIndex: viewingIndex >= 0 ? viewingIndex : selected
    property string visibilityJson: "[]"
    readonly property var visibleIndices: new Set(JSON.parse(visibilityJson))
    property string activityJson: "[]"
    readonly property var activity: JSON.parse(activityJson)
    property string programsJson: "[]"
    property real now: 0
    property string band: "GR"
    readonly property var programs: JSON.parse(programsJson)
    readonly property var filtered: rows.filter(row => row.band === band && (row.index === openingIndex || !visibleIndices.size || visibleIndices.has(row.index)))
    signal selectRequested(int index)
    function resetCursor() {
        if (!list || !filtered)
            return;
        if (wheelInput) wheelInput.cancelGesture();
        list.cancelFlick();
        const index = filtered.findIndex(row => row.index === openingIndex);
        list.currentIndex = index >= 0 ? index : (filtered.length ? 0 : -1);
        Qt.callLater(revealCursor);
    }
    function revealCursor() {
        list.forceLayout();
        if (list.currentIndex >= 0)
            list.positionViewAtIndex(list.currentIndex, ListView.Contain);
    }
    function openBrowser() {
        const current = rows.find(row => row.index === openingIndex) || rows[0];
        if (current)
            band = current.band;
        resetCursor();
    }
    onFilteredChanged: resetCursor()
    onOpeningIndexChanged: openBrowser()
    onRowsChanged: openBrowser()
    Component.onCompleted: openBrowser()
    Flickable {
        id: tabsArea
        anchors {
            left: parent.left
            right: parent.right
            top: parent.top
        }
        height: 40
        contentWidth: Math.max(width, tabs.width)
        contentHeight: height
        clip: true
        BroadcastTabs {
            id: tabs
            x: Math.max(0, (tabsArea.width - width) / 2)
            rows: root.rows
            value: root.band
            onSelected: function (band) {
                root.band = band;
                list.positionViewAtBeginning();
            }
        }
    }
    ListView {
        id: list
        objectName: "sidebarChannelList"
        anchors {
            left: parent.left
            right: parent.right
            top: tabsArea.bottom
            topMargin: 14
            bottom: parent.bottom
        }
        spacing: 12
        clip: true
        cacheBuffer: 0
        model: root.filtered
        onModelChanged: Qt.callLater(root.resetCursor)
        activeFocusOnTab: true
        Keys.onReturnPressed: if (root.filtered[currentIndex])
            root.selectRequested(root.filtered[currentIndex].index)
        Keys.onEnterPressed: if (root.filtered[currentIndex])
            root.selectRequested(root.filtered[currentIndex].index)
        delegate: ItemDelegate {
            id: card
            property real feedbackScale: down ? 0.985 : 1
            Behavior on feedbackScale {
                NumberAnimation { duration: card.down ? 65 : 150; easing.type: Easing.OutCubic }
            }
            hoverEnabled: true
            objectName: "sidebarChannelCard"
            required property var modelData
            required property int index
            readonly property var program: root.programs[modelData.index] || null
            readonly property real progress: program && program.duration > 0 ? Math.max(0, Math.min(1, (root.now - program.startAt) / program.duration)) : 0
            width: ListView.view.width
            height: 150
            padding: 14
            highlighted: modelData.index === root.selected
            onClicked: root.selectRequested(modelData.index)
            background: Rectangle {
                scale: card.feedbackScale
                radius: 14
                color: card.down ? "#344238" : card.highlighted ? "#26302a" : card.hovered ? "#272d28" : "#1c1f1c"
                border.color: card.highlighted || card.hovered || card.visualFocus ? "#9caf9f" : "#24ffffff"
                Behavior on color { ColorAnimation { duration: 120 } }
                Behavior on border.color { ColorAnimation { duration: 120 } }
            }
            contentItem: Item {
                scale: card.feedbackScale
                Row {
                    id: heading
                    spacing: 8
                    height: 32
                    ChannelLogo {
                        width: 56
                        height: 32
                        logoUrl: card.modelData.logo
                    }
                    Item {
                        width: Math.max(0, card.width - 28 - 64 - (activityLabel.visible ? activityLabel.width + 8 : 0))
                        height: 32
                        readonly property int indicatorGap: 6
                        Label {
                            id: channelName
                            anchors.verticalCenter: parent.verticalCenter
                            width: Math.min(implicitWidth, Math.max(0, parent.width
                                - (watching.visible ? watching.width + parent.indicatorGap : 0)))
                            text: card.modelData.label.replace(/^\d+\s+/, "")
                            color: "#f4f5f3"
                            font.bold: true
                            elide: Text.ElideRight
                            textFormat: Text.PlainText
                        }
                        WatchingIndicator {
                            id: watching
                            objectName: "sidebarWatchingIndicator"
                            visible: card.modelData.index === root.viewingIndex
                            x: channelName.width + parent.indicatorGap
                            anchors.verticalCenter: parent.verticalCenter
                        }
                    }
                    Label {
                        id: activityLabel
                        readonly property string force: root.activity[card.modelData.index] ?? ""
                        text: force.length ? qsTranslate("Main", "Activity ") + force : ""
                        visible: text.length > 0
                        height: 32; verticalAlignment: Text.AlignVCenter
                        color: "#9caf9f"; font.pixelSize: 11; font.bold: true
                    }
                }
                Label {
                    id: title
                    anchors {
                        left: parent.left
                        right: parent.right
                        top: heading.bottom
                        topMargin: 8
                    }
                    text: card.program ? (card.program.name || qsTranslate("Viewer", "Program title unavailable")) : qsTranslate("Main", "No program information")
                    color: "#f4f5f3"
                    font.bold: true
                    elide: Text.ElideRight
                    textFormat: Text.PlainText
                }
                Label {
                    id: programTime
                    anchors {
                        left: parent.left
                        right: parent.right
                        top: title.bottom
                        topMargin: 8
                    }
                    text: card.program ? Qt.formatDateTime(new Date(card.program.startAt), "hh:mm") + " – " + Qt.formatDateTime(new Date(card.program.startAt + card.program.duration), "hh:mm") : ""
                    color: "#b6bab6"
                    font.pixelSize: 11
                }
                Label {
                    objectName: "sidebarNextProgram"
                    readonly property var nextProgram: card.program ? card.program.next : null
                    anchors {
                        left: parent.left
                        right: parent.right
                        top: programTime.bottom
                        topMargin: 5
                    }
                    visible: !!nextProgram
                    text: nextProgram ? qsTranslate("Viewer", "Next %1 %2")
                        .arg(Qt.formatDateTime(new Date(nextProgram.startAt), "hh:mm"))
                        .arg(nextProgram.name || qsTranslate("Viewer", "Program title unavailable")) : ""
                    color: "#b6bab6"
                    font.pixelSize: 11
                    textFormat: Text.PlainText
                    elide: Text.ElideRight
                }
                Rectangle {
                    anchors {
                        left: parent.left
                        right: parent.right
                        bottom: parent.bottom
                        bottomMargin: -4
                    }
                    height: 3
                    radius: 2
                    color: "#32ffffff"
                    Rectangle {
                        width: parent.width * card.progress
                        height: parent.height
                        radius: 2
                        color: "#9caf9f"
                    }
                }
            }
        }
    }
    ChannelWheelArea {
        id: wheelInput
        objectName: "sidebarScrollArea"
        anchors.fill: list
        view: list
    }

}
