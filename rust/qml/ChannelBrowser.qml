pragma ComponentBehavior: Bound
import QtQuick
import MinimalViewer
import QtQuick.Controls
import QtQuick.Layouts

Pane {
    id: root
    enum Motion { Positioning, Idle, Scrolling, Snapping }
    required property ChannelModel channels
    required property int selected
    property int viewingIndex: -1
    readonly property int openingIndex: viewingIndex >= 0 ? viewingIndex : selected
    property url iconDirectory: "qrc:/qt/qml/MinimalViewer/assets/icons/"
    property string visibilityJson: "[]"
    readonly property list<int> visibleIndices: JSON.parse(visibilityJson)
    property string activityJson: "[]"
    readonly property var activity: JSON.parse(activityJson)
    property string programsJson: "[]"
    property real now: 0
    readonly property var programs: JSON.parse(programsJson)
    property string band: "GR"
    readonly property alias filteredChannels: channelFilter
    ChannelFilterModel {
        id: channelFilter
        sourceModel: root.channels
        band: root.band
        visible_indices: root.visibleIndices
        visibility: root.visibleIndices.length ? ChannelFilterModel.Listed : ChannelFilterModel.All
        opening_index: root.openingIndex
    }
    // Let ListView apply row removals before restoring the cursor.
    Connections { target: channelFilter; function onChanged() { Qt.callLater(root.resetCursor); } }
    Connections { target: root.channels; function onChanged() { root.restoreSelection(); } }
    signal selectRequested(int index)
    signal closeRequested
    function focusBrowser() {
        list.forceActiveFocus();
    }
    function openBrowser() {
        restoreSelection();
        focusBrowser();
    }
    function restoreSelection() {
        if (!channels)
            return;
        const current = channels.row(Math.max(0, channels.row_for_channel(openingIndex)));
        if (current.band)
            band = current.band;
        resetCursor();
    }
    function resetCursor() {
        // Required inputs may arrive before the derived binding and child view
        // are initialized. Component.onCompleted performs the initial selection.
        if (!list || !channelFilter)
            return;
        const selectedRow = channelFilter.row_for_channel(openingIndex);
        list.resetPosition(selectedRow >= 0 ? selectedRow : (channelFilter.count ? 0 : -1));
    }
    function selectCurrent() {
        const row = channelFilter.row(list.currentIndex);
        if (row.channelIndex !== undefined)
            selectRequested(row.channelIndex);
    }
    onOpeningIndexChanged: restoreSelection()
    onChannelsChanged: restoreSelection()
    Component.onCompleted: openBrowser()
    implicitHeight: 304
    leftPadding: Theme.spaceXl
    rightPadding: Theme.spaceXl
    topPadding: Theme.spaceXl
    bottomPadding: 38
    font.family: "Noto Sans CJK JP"
    background: Rectangle {
        gradient: Gradient {
            GradientStop {
                position: 0
                color: Theme.videoShadeStart
            }
            GradientStop {
                position: 0.35
                color: Theme.videoShadeMiddle
            }
            GradientStop {
                position: 1
                color: Theme.videoShade
            }
        }
    }
    contentItem: ColumnLayout {
        spacing: Theme.spaceLg
        Row {
            Layout.fillWidth: true
            spacing: Theme.spaceLg
            BrowserCollapseButton {
                objectName: "browserCloseButton"
                iconDirectory: root.iconDirectory
                onClicked: root.closeRequested()
            }
            Label {
                text: qsTranslate("Main", "Channels")
                color: Theme.textPrimary
                font.pixelSize: Theme.fontTitle
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
                channels: root.channels
                value: root.band
                directionalNavigation: true
                onDownRequested: root.focusBrowser()
                onSelected: function (band) {
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
                spacing: Theme.spaceLg
                clip: true
                // Keep the extra half-card visible at the viewport edges.
                cacheBuffer: extraCardWidth / 2
                model: channelFilter
                // ListView resets currentIndex when a new filtered model is
                // installed, after the source's change handlers have run.
                onModelChanged: Qt.callLater(root.resetCursor)
                readonly property real baseCardWidth: Math.max(0, Math.min(270, width))
                readonly property real candidateWidth: Math.max(0, Math.min(356, width))
                readonly property real extraCardWidth: Math.max(0, candidateWidth - baseCardWidth)
                readonly property real cardStride: baseCardWidth + spacing
                // Offscreen headers can retain an old x after a resize. The
                // current delegate stays instantiated and anchors the uniform grid.
                readonly property real logicalOrigin: currentItem ? currentItem.x - currentIndex * cardStride : 0
                readonly property real focusPosition: Math.max(0, Math.min(count - 1,
                    (contentX + width / 2 - logicalOrigin - baseCardWidth / 2) / cardStride))
                readonly property int focusLeftIndex: Math.floor(focusPosition)
                readonly property real focusFraction: focusPosition - focusLeftIndex
                readonly property real focusBlend: focusFraction * focusFraction * (3 - 2 * focusFraction)
                readonly property int nearestIndex: count ? Math.round(focusPosition) : -1
                readonly property int snapDurationMs: Theme.moveDuration
                property int motion: ChannelBrowser.Idle
                // The scroll geometry stays fixed. The two central cards share
                // the extra width; shifting their neighbors preserves the gap
                // without feeding visual expansion back into ListView's layout.
                highlightRangeMode: ListView.NoHighlightRange
                highlightFollowsCurrentItem: false
                boundsBehavior: Flickable.StopAtBounds
                maximumFlickVelocity: wheelInput.maximumFlickSpeedPixelsPerSecond
                flickDeceleration: wheelInput.flickDecelerationPixelsPerSecondSquared
                header: Item { width: (list.width - list.baseCardWidth) / 2; height: 1 }
                footer: Item { width: (list.width - list.baseCardWidth) / 2; height: 1 }
                function expansion(index: int): real {
                    return index === focusLeftIndex ? 1 - focusBlend
                        : index === focusLeftIndex + 1 ? focusBlend : 0;
                }
                function cardOffset(index: int): real {
                    const preceding = index <= focusLeftIndex ? 0
                        : index === focusLeftIndex + 1 ? 1 - focusBlend : 1;
                    return extraCardWidth * (preceding - 0.5);
                }
                function centeredPosition(index: int): real {
                    return Math.round(logicalOrigin + index * cardStride + baseCardWidth / 2 - width / 2);
                }
                function keepCentered() {
                    // ListView can adjust contentX in a later resize/layout pass.
                    // Only an idle carousel follows that correction; fresh input
                    // always takes precedence over a queued centering callback.
                    if (motion === ChannelBrowser.Idle && currentIndex >= 0)
                        contentX = centeredPosition(currentIndex);
                }
                function resetPosition(index: int) {
                    motion = ChannelBrowser.Positioning;
                    centerMotion.stop();
                    if (wheelInput) wheelInput.cancelGesture();
                    cancelFlick();
                    currentIndex = index;
                    Qt.callLater(finishPositioning);
                }
                function finishPositioning() {
                    if (motion !== ChannelBrowser.Positioning)
                        return;
                    forceLayout();
                    if (currentIndex >= 0)
                        contentX = centeredPosition(currentIndex);
                    motion = ChannelBrowser.Idle;
                }
                function beginScroll() {
                    motion = ChannelBrowser.Scrolling;
                    centerMotion.stop();
                    currentIndex = nearestIndex;
                }
                function snapTo(index: int) {
                    motion = ChannelBrowser.Snapping;
                    centerMotion.stop();
                    wheelInput.cancelGesture();
                    cancelFlick();
                    currentIndex = index;
                    if (index < 0) {
                        motion = ChannelBrowser.Idle;
                        return;
                    }
                    forceLayout();
                    centerMotion.to = centeredPosition(index);
                    centerMotion.start();
                }
                function finishScroll() {
                    if (visible && enabled && motion === ChannelBrowser.Scrolling && !moving && !wheelInput.active)
                        snapTo(nearestIndex);
                }
                function moveCandidate(offset: int) {
                    if (!count)
                        return;
                    snapTo(Math.max(0, Math.min(count - 1, currentIndex + offset)));
                }
                onNearestIndexChanged: if (motion === ChannelBrowser.Scrolling) currentIndex = nearestIndex
                onContentXChanged: if (motion === ChannelBrowser.Idle) Qt.callLater(keepCentered)
                onLogicalOriginChanged: Qt.callLater(keepCentered)
                onMotionChanged: if (motion === ChannelBrowser.Idle) Qt.callLater(keepCentered)
                onWidthChanged: resetPosition(currentIndex)
                onMovementStarted: beginScroll()
                onMovementEnded: finishScroll()
                onDraggingChanged: if (dragging) wheelInput.cancelGesture()
                onVisibleChanged: if (!visible) { centerMotion.stop(); motion = ChannelBrowser.Idle; }
                onEnabledChanged: if (!enabled) { centerMotion.stop(); motion = ChannelBrowser.Idle; }
                NumberAnimation {
                    id: centerMotion
                    target: list
                    property: "contentX"
                    duration: list.snapDurationMs
                    easing.type: Easing.OutCubic
                    onFinished: list.motion = ChannelBrowser.Idle
                }
                keyNavigationEnabled: false
                keyNavigationWraps: false
                Keys.onLeftPressed: list.moveCandidate(-1)
                Keys.onRightPressed: list.moveCandidate(1)
                Keys.onUpPressed: bands.focusCurrent()
                Keys.onDownPressed: function(event) { event.accepted = true; }
                Keys.onReturnPressed: root.selectCurrent()
                Keys.onEnterPressed: root.selectCurrent()
                ScrollBar.horizontal: ScrollBar {
                    onPressedChanged: {
                        if (pressed) list.beginScroll();
                        else list.finishScroll();
                    }
                }
                delegate: Item {
                    id: slot
                    required property int channelIndex
                    required property string label
                    required property string logo
                    required property int index
                    width: list.baseCardWidth
                    height: 164
                    ItemDelegate {
                        id: card
                        property real feedbackScale: down ? Theme.cardPressScale : 1
                        Behavior on feedbackScale {
                            NumberAnimation { duration: card.down ? Theme.pressDuration : Theme.moveDuration; easing.type: Easing.OutCubic }
                        }
                        hoverEnabled: true
                        objectName: "browserChannelCard"
                        readonly property int channelIndex: slot.channelIndex
                        readonly property string label: slot.label
                        readonly property string logo: slot.logo
                        readonly property int index: slot.index
                        x: list.cardOffset(index)
                        readonly property real expansion: list.expansion(index)
                        width: list.baseCardWidth + list.extraCardWidth * expansion
                        height: 164
                        padding: Theme.spaceLg
                        highlighted: slot.ListView.isCurrentItem
                        onClicked: root.selectRequested(channelIndex)
                        background: CardSurface {
                            scale: card.feedbackScale
                            selected: card.highlighted
                            hovered: card.hovered
                            pressed: card.down
                        }
                        contentItem: Item {
                            scale: card.feedbackScale
                            RowLayout {
                                id: channelHeading
                                width: parent.width
                                height: 32
                                spacing: Theme.spaceSm
                                ChannelLogo {
                                    logoUrl: card.logo || ""
                                    Layout.preferredWidth: 56
                                    Layout.preferredHeight: 32
                                }
                                Item {
                                    Layout.fillWidth: true
                                    Layout.preferredHeight: 32
                                    readonly property int indicatorGap: 6
                                    Label {
                                        id: channelName
                                        objectName: "browserChannelName"
                                        anchors.verticalCenter: parent.verticalCenter
                                        width: Math.min(implicitWidth, Math.max(0, parent.width
                                            - (watching.visible ? watching.width + parent.indicatorGap : 0)))
                                        text: card.label.replace(/^\d+\s+/, "")
                                        color: Theme.textSecondary
                                        font.pixelSize: Theme.fontCaption
                                        textFormat: Text.PlainText
                                        elide: Text.ElideRight
                                    }
                                    WatchingIndicator {
                                        id: watching
                                        objectName: "browserWatchingIndicator"
                                        visible: card.channelIndex === root.viewingIndex
                                        x: channelName.width + parent.indicatorGap
                                        anchors.verticalCenter: parent.verticalCenter
                                    }
                                }
                                Label {
                                    readonly property string force: root.activity[card.channelIndex] ?? ""
                                    text: force.length ? qsTranslate("Main", "Activity ") + force : ""
                                    visible: text.length > 0
                                    color: Theme.accent; font.pixelSize: Theme.fontCaption; font.bold: true
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
                                program: root.programs[card.channelIndex] || null
                                now: root.now
                                emphasized: card.highlighted
                            }
                        }
                        Rectangle {
                            anchors.fill: parent
                            color: "transparent"
                            border.width: 2
                            radius: Theme.panelRadius
                            border.color: Theme.accent
                            visible: list.activeFocus && list.currentIndex === card.index
                        }
                    }
                }
                Label {
                    anchors.centerIn: parent
                    visible: list.count === 0
                    text: qsTranslate("Viewer", "No matching channels")
                    color: Theme.textSecondary
                }
            }
            ChannelWheelArea {
                id: wheelInput
                objectName: "browserScrollArea"
                anchors.fill: parent
                enabled: list.count > 0
                view: list
                orientation: Qt.Horizontal
                pixelInertia: true
                onScrollStarted: {
                    list.beginScroll();
                    list.forceActiveFocus();
                }
                onScrollFinished: list.finishScroll()
            }
        }
    }
}
