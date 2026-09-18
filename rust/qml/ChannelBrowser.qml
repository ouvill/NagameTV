pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Pane {
    id: root
    required property var rows
    required property int selected
    property url iconDirectory: "qrc:/qt/qml/MinimalViewer/assets/icons/"
    property string visibilityJson: "[]"
    readonly property var visibleIndices: new Set(JSON.parse(visibilityJson))
    property string activityJson: "[]"
    readonly property var activity: JSON.parse(activityJson)
    property string programsJson: "[]"
    property real now: 0
    readonly property var programs: JSON.parse(programsJson)
    property string band: "GR"
    property bool navigationAnimated: false
    readonly property var filteredRows: rows.filter(row => row.band === band && (row.index === selected || !visibleIndices.size || visibleIndices.has(row.index)))
    signal selectRequested(int index)
    signal closeRequested
    function focusBrowser() {
        list.forceActiveFocus();
    }
    function openBrowser() {
        restoreSelection();
        Qt.callLater(list.centerCurrent);
        focusBrowser();
    }
    function restoreSelection() {
        navigationAnimated = false;
        if (!rows)
            return;
        const current = rows.find(row => row.index === selected) || rows[0];
        if (current)
            band = current.band;
        resetCursor();
    }
    function resetCursor() {
        if (wheelInput)
            wheelInput.reset();
        // Required inputs may arrive before the derived binding and child view
        // are initialized. Component.onCompleted performs the initial selection.
        if (!list || !filteredRows)
            return;
        const selectedRow = filteredRows.findIndex(row => row.index === selected);
        list.currentIndex = selectedRow >= 0 ? selectedRow : (filteredRows.length ? 0 : -1);
    }
    function selectCurrent() {
        const row = filteredRows[list.currentIndex];
        if (row)
            selectRequested(row.index);
    }
    onFilteredRowsChanged: resetCursor()
    onSelectedChanged: restoreSelection()
    onRowsChanged: restoreSelection()
    Component.onCompleted: openBrowser()
    implicitHeight: 304
    leftPadding: 24
    rightPadding: 24
    topPadding: 20
    bottomPadding: 38
    font.family: "Noto Sans CJK JP"
    background: Rectangle {
        gradient: Gradient {
            GradientStop {
                position: 0
                color: "#06000000"
            }
            GradientStop {
                position: 0.35
                color: "#52000000"
            }
            GradientStop {
                position: 1
                color: "#d6000000"
            }
        }
    }
    contentItem: ColumnLayout {
        spacing: 14
        Row {
            Layout.fillWidth: true
            spacing: 14
            BrowserCollapseButton {
                objectName: "browserCloseButton"
                iconDirectory: root.iconDirectory
                onClicked: root.closeRequested()
            }
            Label {
                text: qsTranslate("Main", "Channels")
                color: "#f4f5f3"
                font.pixelSize: 22
                font.bold: true
                anchors.verticalCenter: parent.verticalCenter
            }
            Item {
                width: 24
                height: 1
            }
            BroadcastTabs {
                id: bands
                objectName: "browserBand"
                anchors.verticalCenter: parent.verticalCenter
                rows: root.rows
                value: root.band
                directionalNavigation: true
                onDownRequested: root.focusBrowser()
                onSelected: function (band) {
                    root.navigationAnimated = true;
                    root.band = band;
                }
            }
        }
        Item {
            Layout.fillWidth: true
            Layout.preferredHeight: 190
            ListView {
                id: list
                objectName: "browserList"
                anchors.fill: parent
                orientation: ListView.Horizontal
                spacing: 14
                clip: true
                cacheBuffer: 0
                model: root.filteredRows
                // ListView resets currentIndex when a new filtered model is
                // installed, after the source's change handlers have run.
                onModelChanged: Qt.callLater(root.resetCursor)
                readonly property real candidateWidth: Math.min(356, width)
                preferredHighlightBegin: (width - candidateWidth) / 2
                preferredHighlightEnd: (width + candidateWidth) / 2
                // Keep keyboard selection independent of scrolling, with room
                // to center even the first and last cards.
                highlightRangeMode: ListView.NoHighlightRange
                highlightFollowsCurrentItem: false
                header: Item { width: list.preferredHighlightBegin; height: 1 }
                footer: Item { width: list.preferredHighlightBegin; height: 1 }
                property real centerOffset: 0
                property bool followingCurrent: true
                // Follow the live layout while easing the remaining distance
                // to the center. Width animation never leaves a stale target.
                function trackCenter() {
                    if (!followingCurrent || !currentItem)
                        return;
                    forceLayout();
                    if (!currentItem)
                        return;
                    contentX = currentItem.x + currentItem.width / 2 - width / 2 + centerOffset;
                }
                function animateCenter() {
                    if (!root.navigationAnimated) {
                        centerCurrent();
                        return;
                    }
                    if (!currentItem)
                        return;
                    centerMotion.stop();
                    followingCurrent = true;
                    forceLayout();
                    if (!currentItem)
                        return;
                    centerOffset = contentX - (currentItem.x + currentItem.width / 2 - width / 2);
                    centerMotion.restart();
                }
                function centerCurrent() {
                    centerMotion.stop();
                    followingCurrent = true;
                    centerOffset = 0;
                    trackCenter();
                }
                onCurrentItemChanged: Qt.callLater(animateCenter)
                onWidthChanged: Qt.callLater(centerCurrent)
                onMovementStarted: {
                    followingCurrent = false;
                    centerMotion.stop();
                }
                NumberAnimation {
                    id: centerMotion
                    target: list
                    property: "centerOffset"
                    to: 0
                    duration: 180
                    easing.type: Easing.OutCubic
                    onRunningChanged: if (!running) Qt.callLater(list.trackCenter)
                }
                FrameAnimation {
                    running: centerMotion.running
                    onTriggered: list.trackCenter()
                }
                keyNavigationEnabled: false
                keyNavigationWraps: false
                Keys.onLeftPressed: {
                    wheelInput.reset();
                    root.navigationAnimated = true;
                    list.decrementCurrentIndex();
                }
                Keys.onRightPressed: {
                    wheelInput.reset();
                    root.navigationAnimated = true;
                    list.incrementCurrentIndex();
                }
                Keys.onUpPressed: bands.focusCurrent()
                Keys.onDownPressed: function(event) { event.accepted = true; }
                Keys.onReturnPressed: root.selectCurrent()
                Keys.onEnterPressed: root.selectCurrent()
                ScrollBar.horizontal: ScrollBar {}
                delegate: Item {
                    id: slot
                    required property var modelData
                    required property int index
                    width: card.width
                    onWidthChanged: {
                        if (!centerMotion.running)
                            Qt.callLater(list.trackCenter);
                    }
                    height: 164
                    ItemDelegate {
                        id: card
                        property real feedbackScale: down ? 0.985 : 1
                        Behavior on feedbackScale {
                            NumberAnimation { duration: card.down ? 65 : 150; easing.type: Easing.OutCubic }
                        }
                        hoverEnabled: true
                        objectName: "browserChannelCard"
                        readonly property var modelData: slot.modelData
                        readonly property int index: slot.index
                        anchors.horizontalCenter: parent.horizontalCenter
                        // Animate selection independently of viewport size so a
                        // resize immediately clamps the card to the available width.
                        property real expansion: highlighted ? 1 : 0
                        Behavior on expansion {
                            enabled: root.navigationAnimated
                            NumberAnimation {
                                duration: 180
                                easing.type: Easing.OutCubic
                            }
                        }
                        width: Math.min(270, list.width)
                            + (list.candidateWidth - Math.min(270, list.width)) * expansion
                        height: 164
                        padding: 14
                        highlighted: slot.ListView.isCurrentItem
                        onClicked: root.selectRequested(modelData.index)
                        background: Rectangle {
                            scale: card.feedbackScale
                            radius: 16
                            color: card.down ? "#344238" : card.highlighted ? "#26302a" : card.hovered ? "#272d28" : "#1c1f1c"
                            border.color: card.highlighted || card.hovered ? "#9caf9f" : "#30ffffff"
                            Behavior on color { ColorAnimation { duration: 120 } }
                            Behavior on border.color { ColorAnimation { duration: 120 } }
                        }
                        contentItem: Item {
                            scale: card.feedbackScale
                            RowLayout {
                                id: channelHeading
                                width: parent.width
                                height: 32
                                spacing: 8
                                ChannelLogo {
                                    logoUrl: card.modelData.logo || ""
                                    Layout.preferredWidth: 56
                                    Layout.preferredHeight: 32
                                }
                                Label {
                                    Layout.fillWidth: true
                                    text: card.modelData.label.replace(/^\d+\s+/, "")
                                    color: "#b6bab6"
                                    font.pixelSize: 12
                                    textFormat: Text.PlainText
                                    elide: Text.ElideRight
                                }
                                Label {
                                    readonly property string force: root.activity[card.modelData.index] ?? ""
                                    text: force.length ? qsTranslate("Main", "Activity ") + force : ""
                                    visible: text.length > 0
                                    color: "#9caf9f"; font.pixelSize: 11; font.bold: true
                                }
                            }
                            ChannelProgram {
                                anchors {
                                    top: channelHeading.bottom
                                    topMargin: 9
                                    left: parent.left
                                    right: parent.right
                                    bottom: parent.bottom
                                    bottomMargin: -6
                                }
                                program: root.programs[card.modelData.index] || null
                                now: root.now
                                emphasized: card.highlighted
                            }
                        }
                        Rectangle {
                            anchors.fill: parent
                            color: "transparent"
                            border.width: 2
                            radius: 16
                            border.color: "#9caf9f"
                            visible: list.activeFocus && list.currentIndex === card.index
                        }
                    }
                }
                Label {
                    anchors.centerIn: parent
                    visible: list.count === 0
                    text: qsTranslate("Viewer", "No matching channels")
                    color: "#cccccc"
                }
            }
            ChannelScrollArea {
                id: wheelInput
                objectName: "browserScrollArea"
                anchors.fill: parent
                enabled: list.count > 0
                mode: ChannelScrollArea.Steps
                pixelsPerStep: list.candidateWidth + list.spacing
                onScrolled: function(steps) {
                    list.forceActiveFocus();
                    root.navigationAnimated = true;
                    list.currentIndex = Math.max(0, Math.min(list.count - 1, list.currentIndex + steps));
                }
            }
        }
    }
}
