pragma ComponentBehavior: Bound
import QtQuick
import MinimalViewer
import QtQuick.Controls
import QtQuick.Layouts

// What is on air, shown while nothing plays. The program header above and the
// playback controls below belong to the viewer; this fills the space between.
Item {
    id: root
    required property ChannelModel channels
    required property int selected
    property string visibilityJson: "[]"
    readonly property list<int> visibleIndices: JSON.parse(visibilityJson)
    property string programsJson: "[]"
    readonly property var programs: JSON.parse(programsJson)
    property real now: 0
    property bool loading: false
    property string band: "GR"
    readonly property real headerReserve: 150
    readonly property real controlsReserve: 116
    readonly property real minimumCardWidth: 264
    readonly property real cardHeight: 148
    signal playRequested
    signal selectRequested(int index)
    signal recordingsRequested
    signal openFileRequested
    ChannelFilterModel {
        id: channelFilter
        sourceModel: root.channels
        band: root.band
        visible_indices: root.visibleIndices
        visibility: root.visibleIndices.length ? ChannelFilterModel.Listed : ChannelFilterModel.All
        opening_index: root.selected
    }
    function restoreBand() {
        const current = channels.row(Math.max(0, channels.row_for_channel(selected)));
        if (current.band) band = current.band;
    }
    function focusChannels() {
        if (grid.count === 0) return;
        if (grid.currentIndex < 0) grid.currentIndex = Math.max(0, channelFilter.row_for_channel(selected));
        grid.forceActiveFocus(Qt.TabFocusReason);
    }
    function selectCurrent() {
        const row = channelFilter.row(grid.currentIndex);
        if (row.channelIndex !== undefined) selectRequested(row.channelIndex);
    }
    Connections { target: root.channels; function onChanged() { root.restoreBand(); } }
    Component.onCompleted: restoreBand()
    ColumnLayout {
        anchors {
            fill: parent
            leftMargin: Theme.spaceXl; rightMargin: Theme.spaceXl
            topMargin: root.headerReserve; bottomMargin: root.controlsReserve
        }
        spacing: Theme.spaceLg
        DirectionalFocus {
            id: actions
            objectName: "overviewActions"
            Layout.fillWidth: true
            implicitHeight: actionRow.implicitHeight
            navigationItems: [watch, recordings, openFile]
            initialItem: watch
            onBoundaryReached: function(key) {
                if (key === Qt.Key_Down) {
                    if (bands.visible) bands.focusCurrent();
                    else root.focusChannels();
                }
            }
            RowLayout {
                id: actionRow
                anchors.fill: parent
                spacing: Theme.spaceMd
                ActionButton {
                    id: watch
                    objectName: "overviewWatch"
                    emphasis: ActionButton.Primary
                    Layout.minimumWidth: 160
                    visible: root.selected >= 0
                    enabled: !root.loading
                    text: qsTranslate("Main", "Watch")
                    onClicked: root.playRequested()
                }
                ActionButton {
                    id: recordings
                    objectName: "overviewRecordings"
                    text: qsTranslate("RecordingLibrary", "Recordings")
                    onClicked: root.recordingsRequested()
                }
                ActionButton {
                    id: openFile
                    objectName: "overviewOpenFile"
                    text: qsTranslate("Recording", "Open video file")
                    onClicked: root.openFileRequested()
                }
                Item { Layout.fillWidth: true }
                BusyIndicator {
                    objectName: "overviewLoading"
                    implicitWidth: Theme.controlHeight
                    implicitHeight: Theme.controlHeight
                    visible: root.loading
                    running: visible
                    palette.dark: Theme.accent
                    palette.text: Theme.accent
                    Accessible.name: qsTranslate("Viewer", "Loading…")
                }
            }
        }
        RowLayout {
            Layout.fillWidth: true
            spacing: Theme.spaceLg
            Label {
                text: qsTranslate("Viewer", "On air now")
                color: Theme.textPrimary
                font.pixelSize: Theme.fontHeading
                font.bold: true
            }
            BroadcastTabs {
                id: bands
                objectName: "overviewBand"
                Layout.preferredWidth: implicitWidth
                channels: root.channels
                value: root.band
                directionalNavigation: true
                onUpRequested: actions.enter()
                onDownRequested: root.focusChannels()
                onSelected: function(band) { root.band = band; grid.currentIndex = -1; }
            }
            Item { Layout.fillWidth: true }
        }
        GridView {
            id: grid
            objectName: "overviewChannels"
            Layout.fillWidth: true
            Layout.fillHeight: true
            readonly property int columns: Math.max(1, Math.floor((width + Theme.spaceMd) / (root.minimumCardWidth + Theme.spaceMd)))
            cellWidth: (width + Theme.spaceMd) / columns
            cellHeight: root.cardHeight + Theme.spaceMd
            clip: true
            currentIndex: -1
            boundsBehavior: Flickable.StopAtBounds
            keyNavigationEnabled: false
            activeFocusOnTab: true
            model: channelFilter
            ScrollBar.vertical: ScrollBar {}
            function step(offset: int): bool {
                const next = currentIndex + offset;
                if (next < 0 || next >= count) return false;
                currentIndex = next;
                positionViewAtIndex(next, GridView.Contain);
                return true;
            }
            Keys.onLeftPressed: if (currentIndex % columns > 0) step(-1)
            Keys.onRightPressed: if (currentIndex % columns < columns - 1) step(1)
            Keys.onUpPressed: {
                if (step(-columns)) return;
                if (bands.visible) bands.focusCurrent();
                else actions.enter();
            }
            Keys.onDownPressed: function(event) {
                // The last row may be short; land on its final card.
                if (!step(columns) && Math.floor(currentIndex / columns) < Math.floor((count - 1) / columns))
                    step(count - 1 - currentIndex);
                event.accepted = true;
            }
            Keys.onReturnPressed: function(event) { if (!event.isAutoRepeat) root.selectCurrent(); }
            Keys.onEnterPressed: function(event) { if (!event.isAutoRepeat) root.selectCurrent(); }
            delegate: Item {
                id: slot
                required property int channelIndex
                required property string label
                required property string logo
                required property int index
                width: grid.cellWidth
                height: grid.cellHeight
                ItemDelegate {
                    id: card
                    objectName: "overviewChannelCard"
                    readonly property int channelIndex: slot.channelIndex
                    // "Watch" plays this channel; it keeps the selected surface without focus.
                    readonly property bool current: slot.channelIndex === root.selected
                    property real feedbackScale: down ? Theme.cardPressScale : 1
                    Behavior on feedbackScale {
                        NumberAnimation { duration: card.down ? Theme.pressDuration : Theme.moveDuration; easing.type: Easing.OutCubic }
                    }
                    width: slot.width - Theme.spaceMd
                    height: root.cardHeight
                    padding: Theme.spaceLg
                    hoverEnabled: true
                    highlighted: grid.activeFocus && slot.GridView.isCurrentItem
                    Accessible.name: slot.label
                    onClicked: {
                        grid.currentIndex = slot.index;
                        root.selectRequested(slot.channelIndex);
                    }
                    background: CardSurface {
                        scale: card.feedbackScale
                        selected: card.highlighted || card.current
                        hovered: card.hovered
                        pressed: card.down
                        border.width: card.highlighted ? Theme.focusWidth : 1
                    }
                    contentItem: Item {
                        scale: card.feedbackScale
                        RowLayout {
                            id: heading
                            width: parent.width
                            height: 28
                            spacing: Theme.spaceSm
                            ChannelLogo {
                                logoUrl: slot.logo || ""
                                Layout.preferredWidth: 50
                                Layout.preferredHeight: 28
                            }
                            Label {
                                Layout.fillWidth: true
                                text: slot.label.replace(/^\d+\s+/, "")
                                color: Theme.textSecondary
                                font.pixelSize: Theme.fontCaption
                                textFormat: Text.PlainText
                                elide: Text.ElideRight
                            }
                        }
                        ChannelProgram {
                            anchors {
                                top: heading.bottom; topMargin: Theme.spaceSm
                                left: parent.left; right: parent.right; bottom: parent.bottom
                            }
                            program: root.programs[slot.channelIndex] || null
                            now: root.now
                            emphasized: true
                        }
                    }
                }
            }
            Label {
                anchors.centerIn: parent
                visible: grid.count === 0
                text: qsTranslate("Viewer", "No matching channels")
                color: Theme.textSecondary
            }
        }
    }
}
