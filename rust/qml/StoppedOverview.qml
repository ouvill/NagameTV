pragma ComponentBehavior: Bound
import QtQuick
import MinimalViewer
import QtQuick.Controls
import QtQuick.Layouts

// Home: what can be watched, shown while nothing plays. The program header above
// and the playback controls below belong to the viewer; this fills the space between.
Item {
    id: root
    required property ChannelModel channels
    required property int selected
    // Recordings stopped part-way. Absent when the owner has no viewing history.
    property ContinueWatchingModel continueWatching: null
    // Favorites need the persisted history; cards hide the star until it is loaded.
    property bool favoritesReady: false
    // Favorites and positions were not read or saved; the cause is in the log.
    property bool historyFailed: false
    property string visibilityJson: "[]"
    readonly property list<int> visibleIndices: JSON.parse(visibilityJson)
    property string programsJson: "[]"
    readonly property var programs: JSON.parse(programsJson)
    property real now: 0
    property bool loading: false
    property string band: "GR"
    readonly property real headerReserve: 168
    readonly property real controlsReserve: 116
    readonly property real minimumCardWidth: 264
    readonly property real cardHeight: 180
    readonly property real continueCardWidth: 248
    readonly property real continueCardHeight: 232
    // Rows bleed to the window edge; their content starts at the page margin.
    readonly property real pageMargin: Theme.spaceXl
    signal playRequested
    signal selectRequested(int index)
    signal recordingsRequested
    signal openFileRequested
    signal favoriteRequested(string serviceId)
    signal resumeRequested(string recordingKey)
    // The directional keys left the page upward or downward.
    signal boundaryReached(int key)
    function enterTop() { enterStop(actions); }
    function enterBottom() {
        for (let index = stops.length - 1; index >= 0; --index) {
            if (usable(stops[index])) { enterStop(stops[index]); return; }
        }
    }
    ChannelFilterModel {
        id: channelFilter
        sourceModel: root.channels
        band: root.band
        visible_indices: root.visibleIndices
        visibility: root.visibleIndices.length ? ChannelFilterModel.Listed : ChannelFilterModel.All
        opening_index: root.selected
    }
    ChannelFilterModel {
        id: favoriteFilter
        sourceModel: root.channels
        favorites_only: true
    }
    function restoreBand() {
        const current = channels.row(Math.max(0, channels.row_for_channel(selected)));
        if (current.band) band = current.band;
    }
    function focusChannels() {
        if (grid.count === 0) return;
        if (grid.currentIndex < 0) grid.currentIndex = Math.max(0, channelFilter.row_for_channel(selected));
        grid.forceActiveFocus(Qt.TabFocusReason);
        scroller.reveal(grid.currentItem);
    }
    function selectCurrent() {
        const row = channelFilter.row(grid.currentIndex);
        if (row.channelIndex !== undefined) selectRequested(row.channelIndex);
    }
    // The vertical order of the focusable regions. Empty rows are skipped.
    readonly property var stops: [actions, continueList, favoriteList, bands, grid]
    function usable(stop): bool {
        if (stop === actions) return true;
        if (stop === bands) return bands.visible;
        return stop.visible && stop.count > 0;
    }
    function enterStop(stop) {
        if (stop === actions) { actions.enter(); scroller.reveal(actions); }
        else if (stop === bands) { bands.focusCurrent(); scroller.reveal(onAirHeading); }
        else if (stop === grid) focusChannels();
        else {
            if (stop.currentIndex < 0) stop.currentIndex = 0;
            stop.forceActiveFocus(Qt.TabFocusReason);
            scroller.reveal(stop);
        }
    }
    function leave(stop, offset: int): bool {
        for (let index = stops.indexOf(stop) + offset; index >= 0 && index < stops.length; index += offset) {
            if (usable(stops[index])) { enterStop(stops[index]); return true; }
        }
        return false;
    }
    Connections { target: root.channels; function onChanged() { root.restoreBand(); } }
    Component.onCompleted: restoreBand()

    component SectionTitle: Label {
        color: Theme.textPrimary
        font.pixelSize: Theme.fontHeading
        font.bold: true
    }
    // A horizontal row whose cursor follows the directional keys.
    component CardRow: ListView {
        id: row
        orientation: ListView.Horizontal
        spacing: Theme.spaceMd
        leftMargin: root.pageMargin
        rightMargin: root.pageMargin
        currentIndex: -1
        boundsBehavior: Flickable.StopAtBounds
        keyNavigationEnabled: false
        activeFocusOnTab: true
        highlightMoveDuration: Theme.moveDuration
        signal activated(int index)
        function step(offset: int) {
            const next = currentIndex + offset;
            if (next < 0 || next >= count) return;
            currentIndex = next;
            positionViewAtIndex(next, ListView.Contain);
        }
        Keys.onLeftPressed: step(-1)
        Keys.onRightPressed: step(1)
        Keys.onUpPressed: root.leave(row, -1)
        Keys.onDownPressed: function(event) { root.leave(row, 1); event.accepted = true; }
        Keys.onReturnPressed: function(event) { if (!event.isAutoRepeat) row.activated(row.currentIndex); }
        Keys.onEnterPressed: function(event) { if (!event.isAutoRepeat) row.activated(row.currentIndex); }
    }

    Flickable {
        id: scroller
        objectName: "overviewScroll"
        anchors { fill: parent; topMargin: root.headerReserve; bottomMargin: root.controlsReserve }
        contentWidth: width
        contentHeight: page.implicitHeight + Theme.spaceXl
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ScrollBar {}
        // Bring a focused region into view. Pointer scrolling stays untouched.
        function reveal(item: Item) {
            if (!item) return;
            const top = item.mapToItem(page, 0, 0).y;
            const lead = Theme.controlHeight + Theme.spaceLg; // keeps the row's title in view
            let target = contentY;
            if (top - lead < target) target = top - lead;
            else if (top + item.height + Theme.spaceLg > target + height) target = top + item.height + Theme.spaceLg - height;
            target = Math.max(0, Math.min(Math.max(0, contentHeight - height), target));
            if (Math.abs(target - contentY) < 1) return;
            follow.stop();
            follow.to = target;
            follow.start();
        }
        onMovementStarted: follow.stop()
        NumberAnimation {
            id: follow
            target: scroller
            property: "contentY"
            duration: Theme.moveDuration
            easing.type: Easing.OutCubic
        }
        ColumnLayout {
            id: page
            width: scroller.width
            spacing: Theme.spaceLg
            DirectionalFocus {
                id: actions
                objectName: "overviewActions"
                Layout.fillWidth: true
                Layout.leftMargin: root.pageMargin
                Layout.rightMargin: root.pageMargin
                Layout.topMargin: Theme.focusOutset
                implicitHeight: actionRow.implicitHeight
                navigationItems: [watch, recordings, openFile]
                initialItem: watch
                onBoundaryReached: function(key) {
                    if (key === Qt.Key_Down) root.leave(actions, 1);
                    else if (key === Qt.Key_Up) root.boundaryReached(key);
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
            Label {
                objectName: "overviewHistoryWarning"
                Layout.fillWidth: true
                Layout.leftMargin: root.pageMargin
                Layout.rightMargin: root.pageMargin
                visible: root.historyFailed
                text: qsTranslate("Viewer", "Favorites and playback positions could not be saved. They are kept only until the app closes.")
                color: Theme.warning
                font.pixelSize: Theme.fontBody
                wrapMode: Text.Wrap
            }
            SectionTitle {
                Layout.leftMargin: root.pageMargin
                Layout.topMargin: Theme.spaceSm
                visible: continueList.visible
                text: qsTranslate("Viewer", "Continue watching")
            }
            CardRow {
                id: continueList
                objectName: "overviewContinue"
                Layout.fillWidth: true
                Layout.preferredHeight: root.continueCardHeight
                visible: root.continueWatching !== null && count > 0
                model: root.continueWatching
                onActivated: function(index) {
                    const item = itemAtIndex(index) as ContinueCard;
                    if (item) root.resumeRequested(item.recordingKey);
                }
                delegate: ContinueCard {
                    id: resume
                    required property int index
                    objectName: "overviewContinueCard"
                    width: root.continueCardWidth
                    height: root.continueCardHeight
                    highlighted: continueList.activeFocus && ListView.isCurrentItem
                    onClicked: {
                        continueList.currentIndex = resume.index;
                        root.resumeRequested(resume.recordingKey);
                    }
                }
            }
            SectionTitle {
                Layout.leftMargin: root.pageMargin
                Layout.topMargin: Theme.spaceSm
                visible: favoriteList.visible
                text: qsTranslate("Viewer", "Favorites")
            }
            CardRow {
                id: favoriteList
                objectName: "overviewFavorites"
                Layout.fillWidth: true
                Layout.preferredHeight: root.cardHeight
                visible: count > 0
                model: favoriteFilter
                onActivated: function(index) {
                    const row = favoriteFilter.row(index);
                    if (row.channelIndex !== undefined) root.selectRequested(row.channelIndex);
                }
                Keys.onPressed: function(event) {
                    if (event.key !== Qt.Key_F || event.modifiers !== Qt.NoModifier || event.isAutoRepeat) return;
                    const row = favoriteFilter.row(currentIndex);
                    if (row.serviceId !== undefined) root.favoriteRequested(row.serviceId);
                    event.accepted = true;
                }
                delegate: ChannelCard {
                    id: favoriteCard
                    required property int index
                    required property string serviceId
                    objectName: "overviewFavoriteCard"
                    width: root.minimumCardWidth
                    height: root.cardHeight
                    program: root.programs[channelIndex] || null
                    now: root.now
                    current: channelIndex === root.selected
                    favorite: true
                    favoriteEnabled: root.favoritesReady
                    highlighted: favoriteList.activeFocus && ListView.isCurrentItem
                    onFavoriteRequested: root.favoriteRequested(serviceId)
                    onClicked: {
                        favoriteList.currentIndex = favoriteCard.index;
                        root.selectRequested(channelIndex);
                    }
                }
            }
            RowLayout {
                id: onAirHeading
                Layout.fillWidth: true
                Layout.leftMargin: root.pageMargin
                Layout.rightMargin: root.pageMargin
                Layout.topMargin: Theme.spaceSm
                spacing: Theme.spaceLg
                SectionTitle { text: qsTranslate("Viewer", "On air now") }
                BroadcastTabs {
                    id: bands
                    objectName: "overviewBand"
                    Layout.preferredWidth: implicitWidth
                    channels: root.channels
                    value: root.band
                    directionalNavigation: true
                    onUpRequested: root.leave(bands, -1)
                    onDownRequested: root.leave(bands, 1)
                    onSelected: function(band) { root.band = band; grid.currentIndex = -1; }
                }
                Item { Layout.fillWidth: true }
            }
            GridView {
                id: grid
                objectName: "overviewChannels"
                Layout.fillWidth: true
                Layout.leftMargin: root.pageMargin
                Layout.rightMargin: root.pageMargin - Theme.spaceMd
                // The page scrolls; the grid shows every card of the band.
                Layout.preferredHeight: contentHeight
                readonly property int columns: Math.max(1, Math.floor(width / (root.minimumCardWidth + Theme.spaceMd)))
                cellWidth: width / columns
                cellHeight: root.cardHeight + Theme.spaceMd
                interactive: false
                currentIndex: -1
                keyNavigationEnabled: false
                activeFocusOnTab: true
                model: channelFilter
                function step(offset: int): bool {
                    const next = currentIndex + offset;
                    if (next < 0 || next >= count) return false;
                    currentIndex = next;
                    scroller.reveal(currentItem);
                    return true;
                }
                function toggleFavorite() {
                    const row = channelFilter.row(currentIndex);
                    if (row.serviceId !== undefined) root.favoriteRequested(row.serviceId);
                }
                Keys.onLeftPressed: if (currentIndex % columns > 0) step(-1)
                Keys.onRightPressed: if (currentIndex % columns < columns - 1) step(1)
                Keys.onUpPressed: if (!step(-columns)) root.leave(grid, -1)
                Keys.onDownPressed: function(event) {
                    // The last row may be short; land on its final card.
                    if (!step(columns)) {
                        if (Math.floor(currentIndex / columns) < Math.floor((count - 1) / columns)) step(count - 1 - currentIndex);
                        else root.boundaryReached(Qt.Key_Down);
                    }
                    event.accepted = true;
                }
                Keys.onReturnPressed: function(event) { if (!event.isAutoRepeat) root.selectCurrent(); }
                Keys.onEnterPressed: function(event) { if (!event.isAutoRepeat) root.selectCurrent(); }
                Keys.onPressed: function(event) {
                    if (event.key !== Qt.Key_F || event.modifiers !== Qt.NoModifier || event.isAutoRepeat) return;
                    if (root.favoritesReady) toggleFavorite();
                    event.accepted = true;
                }
                delegate: Item {
                    id: slot
                    // The view hands its focus to the current cell.
                    objectName: "overviewChannelCell"
                    required property int channelIndex
                    required property string label
                    required property string logo
                    required property string serviceId
                    required property bool favorite
                    required property int index
                    width: grid.cellWidth
                    height: grid.cellHeight
                    z: card.highlighted ? 1 : 0
                    ChannelCard {
                        id: card
                        objectName: "overviewChannelCard"
                        channelIndex: slot.channelIndex
                        label: slot.label
                        logo: slot.logo
                        width: slot.width - Theme.spaceMd
                        height: root.cardHeight
                        program: root.programs[slot.channelIndex] || null
                        now: root.now
                        current: slot.channelIndex === root.selected
                        favorite: slot.favorite
                        favoriteEnabled: root.favoritesReady
                        highlighted: grid.activeFocus && slot.GridView.isCurrentItem
                        onFavoriteRequested: root.favoriteRequested(slot.serviceId)
                        onClicked: {
                            grid.currentIndex = slot.index;
                            root.selectRequested(slot.channelIndex);
                        }
                    }
                }
            }
            Label {
                Layout.alignment: Qt.AlignHCenter
                Layout.topMargin: Theme.spaceXl
                visible: grid.count === 0
                text: qsTranslate("Viewer", "No matching channels")
                color: Theme.textSecondary
            }
        }
    }
}
