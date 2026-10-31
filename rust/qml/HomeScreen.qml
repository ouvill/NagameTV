pragma ComponentBehavior: Bound
import QtQuick
import MinimalViewer
import QtQuick.Controls
import QtQuick.Layouts

// Home under screen navigation: channels first, then recordings to resume, and a bar
// for what keeps playing behind. The owner draws the mode buttons over the header.
Rectangle {
    id: root
    required property ChannelModel channels
    required property int selected
    // The channel being watched; marks its card and opens its band.
    property int viewingIndex: -1
    property ContinueWatchingModel continueWatching: null
    property bool favoritesReady: false
    property bool historyFailed: false
    property string visibilityJson: "[]"
    readonly property list<int> visibleIndices: JSON.parse(visibilityJson)
    property string programsJson: "[]"
    readonly property var programs: JSON.parse(programsJson)
    property real now: 0
    property bool loading: false
    // What plays behind home. The bar shows only while there is something to return to.
    property bool playing: false
    property Item previewSource: null
    property bool previewAvailable: false
    property string playingChannel: ""
    property string playingLogo: ""
    property string playingTitle: ""
    property string playingProgramJson: "null"
    property ViewerActions actions: null
    property url iconDirectory: "qrc:/qt/qml/MinimalViewer/assets/icons/"
    readonly property real headerHeight: 78
    readonly property real minimumCardWidth: 264
    readonly property real cardHeight: 180
    readonly property real continueCardWidth: 248
    readonly property real continueCardHeight: 232
    readonly property real pageMargin: Theme.spaceXl
    // "FAV" lists favorites across bands; other values are broadcast bands.
    property string band: "GR"
    signal selectRequested(int index)
    signal favoriteRequested(string serviceId)
    signal resumeRequested(string recordingKey)
    signal returnRequested
    // The directional keys left the page upward, toward the mode buttons.
    signal boundaryReached(int key)
    color: Theme.canvas

    ChannelFilterModel {
        id: channelFilter
        sourceModel: root.channels
        band: root.band === "FAV" ? "ALL" : root.band
        favorites_only: root.band === "FAV"
        visible_indices: root.visibleIndices
        visibility: root.visibleIndices.length ? ChannelFilterModel.Listed : ChannelFilterModel.All
        opening_index: root.anchorChannel
    }
    ChannelFilterModel {
        id: favoriteFilter
        sourceModel: root.channels
        band: "ALL"
        favorites_only: true
    }
    readonly property int anchorChannel: viewingIndex >= 0 ? viewingIndex : selected
    // Favorites open first when the channel to land on is one of them.
    function restoreBand() {
        const current = channels.row(Math.max(0, channels.row_for_channel(anchorChannel)));
        if (favoriteFilter.count > 0 && (anchorChannel < 0 || current.favorite === true)) band = "FAV";
        else if (current.band) band = current.band;
    }
    Component.onCompleted: restoreBand()

    // Vertical order of the focusable rows; empty rows are skipped.
    readonly property var stops: [bands, grid, continueList, nowPlaying]
    function usable(stop): bool {
        if (stop === bands) return bands.visible;
        if (stop === nowPlaying) return nowPlaying.visible;
        return stop.visible && stop.count > 0;
    }
    // Moving between rows keeps the column where the next row has one.
    function enterStop(stop, column: int, fromBelow: bool) {
        if (stop === bands) { bands.focusCurrent(); scroller.reveal(onAirHeading); }
        else if (stop === grid) {
            if (column >= 0 && fromBelow) {
                const lastRow = Math.floor((grid.count - 1) / grid.columns) * grid.columns;
                grid.currentIndex = Math.min(grid.count - 1, lastRow + column);
            } else if (column >= 0) grid.currentIndex = Math.min(grid.count - 1, column);
            focusChannels();
        } else if (stop === continueList) {
            if (column >= 0) continueList.currentIndex = Math.min(continueList.count - 1, column);
            else if (continueList.currentIndex < 0) continueList.currentIndex = 0;
            continueList.positionViewAtIndex(continueList.currentIndex, ListView.Contain);
            continueList.forceActiveFocus(Qt.TabFocusReason);
            scroller.reveal(continueList);
        } else nowPlaying.enter();
    }
    function leave(stop, offset: int, column: int): bool {
        for (let index = stops.indexOf(stop) + offset; index >= 0 && index < stops.length; index += offset) {
            if (usable(stops[index])) { enterStop(stops[index], column, offset < 0); return true; }
        }
        if (offset < 0) boundaryReached(Qt.Key_Up);
        return false;
    }
    // From the mode buttons above.
    function enterTop() {
        for (const stop of stops) if (usable(stop)) { enterStop(stop, -1, false); return; }
    }
    function enterBottom() {
        for (let index = stops.length - 1; index >= 0; --index)
            if (usable(stops[index])) { enterStop(stops[index], -1, true); return; }
    }
    // Opening home lands on the channel being watched, or the first card.
    function focusChannels() {
        if (grid.count === 0) { enterTop(); return; }
        if (grid.currentIndex < 0) grid.currentIndex = Math.max(0, channelFilter.row_for_channel(anchorChannel));
        grid.forceActiveFocus(Qt.TabFocusReason);
        scroller.reveal(grid.currentItem);
    }
    function selectCurrent() {
        const row = channelFilter.row(grid.currentIndex);
        if (row.channelIndex !== undefined) selectRequested(row.channelIndex);
    }

    component SectionTitle: Label {
        color: Theme.textPrimary
        font.pixelSize: Theme.fontHeading
        font.bold: true
    }

    Label {
        objectName: "homeTitle"
        anchors { left: parent.left; leftMargin: root.pageMargin; verticalCenter: parent.top; verticalCenterOffset: root.headerHeight / 2 }
        text: qsTranslate("Main", "Home")
        color: Theme.textPrimary
        font.pixelSize: Theme.fontTitle
        font.bold: true
    }
    Flickable {
        id: scroller
        objectName: "homeScroll"
        anchors {
            left: parent.left; right: parent.right; top: parent.top; topMargin: root.headerHeight
            bottom: nowPlaying.visible ? nowPlaying.top : parent.bottom
        }
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
            Label {
                objectName: "homeHistoryWarning"
                Layout.fillWidth: true
                Layout.leftMargin: root.pageMargin
                Layout.rightMargin: root.pageMargin
                visible: root.historyFailed
                text: qsTranslate("Viewer", "Favorites and playback positions could not be saved. They are kept only until the app closes.")
                color: Theme.warning
                font.pixelSize: Theme.fontBody
                wrapMode: Text.Wrap
            }
            RowLayout {
                id: onAirHeading
                Layout.fillWidth: true
                Layout.leftMargin: root.pageMargin
                Layout.rightMargin: root.pageMargin
                Layout.topMargin: Theme.focusOutset
                spacing: Theme.spaceLg
                SectionTitle { text: qsTranslate("Viewer", "On air now") }
                BroadcastTabs {
                    id: bands
                    objectName: "homeBand"
                    Layout.preferredWidth: implicitWidth
                    channels: root.channels
                    favorites: favoriteFilter.count > 0
                    value: root.band
                    directionalNavigation: true
                    onUpRequested: root.leave(bands, -1, -1)
                    onDownRequested: root.leave(bands, 1, -1)
                    onSelected: function(band) { root.band = band; grid.currentIndex = -1; }
                }
                Item { Layout.fillWidth: true }
                BusyIndicator {
                    objectName: "homeLoading"
                    implicitWidth: Theme.controlHeight
                    implicitHeight: Theme.controlHeight
                    visible: root.loading
                    running: visible
                    palette.dark: Theme.accent
                    palette.text: Theme.accent
                    Accessible.name: qsTranslate("Viewer", "Loading…")
                }
            }
            GridView {
                id: grid
                objectName: "homeChannels"
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
                Keys.onUpPressed: if (!step(-columns)) root.leave(grid, -1, -1)
                Keys.onDownPressed: function(event) {
                    // The last row may be short; land on its final card before leaving.
                    if (!step(columns)) {
                        if (Math.floor(currentIndex / columns) < Math.floor((count - 1) / columns)) step(count - 1 - currentIndex);
                        else root.leave(grid, 1, currentIndex % columns);
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
                    objectName: "homeChannelCell"
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
                        objectName: "homeChannelCard"
                        channelIndex: slot.channelIndex
                        label: slot.label
                        logo: slot.logo
                        width: slot.width - Theme.spaceMd
                        height: root.cardHeight
                        program: root.programs[slot.channelIndex] || null
                        now: root.now
                        current: slot.channelIndex === root.anchorChannel
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
            SectionTitle {
                Layout.leftMargin: root.pageMargin
                Layout.topMargin: Theme.spaceSm
                visible: continueList.visible
                text: qsTranslate("Viewer", "Continue watching")
            }
            ListView {
                id: continueList
                objectName: "homeContinue"
                Layout.fillWidth: true
                Layout.preferredHeight: root.continueCardHeight
                visible: root.continueWatching !== null && count > 0
                model: root.continueWatching
                orientation: ListView.Horizontal
                spacing: Theme.spaceMd
                leftMargin: root.pageMargin
                rightMargin: root.pageMargin
                currentIndex: -1
                boundsBehavior: Flickable.StopAtBounds
                keyNavigationEnabled: false
                activeFocusOnTab: true
                highlightMoveDuration: Theme.moveDuration
                function step(offset: int) {
                    const next = currentIndex + offset;
                    if (next < 0 || next >= count) return;
                    currentIndex = next;
                    positionViewAtIndex(next, ListView.Contain);
                }
                function activate() {
                    const item = itemAtIndex(currentIndex) as ContinueCard;
                    if (item) root.resumeRequested(item.recordingKey);
                }
                Keys.onLeftPressed: step(-1)
                Keys.onRightPressed: step(1)
                Keys.onUpPressed: root.leave(continueList, -1, currentIndex)
                Keys.onDownPressed: function(event) { root.leave(continueList, 1, currentIndex); event.accepted = true; }
                Keys.onReturnPressed: function(event) { if (!event.isAutoRepeat) activate(); }
                Keys.onEnterPressed: function(event) { if (!event.isAutoRepeat) activate(); }
                delegate: ContinueCard {
                    id: resume
                    required property int index
                    objectName: "homeContinueCard"
                    width: root.continueCardWidth
                    height: root.continueCardHeight
                    highlighted: continueList.activeFocus && ListView.isCurrentItem
                    onClicked: {
                        continueList.currentIndex = resume.index;
                        root.resumeRequested(resume.recordingKey);
                    }
                }
            }
        }
    }
    // What keeps playing behind home, one step from going back to it.
    Rectangle {
        id: nowPlayingFrame
        anchors { left: parent.left; right: parent.right; bottom: parent.bottom }
        height: nowPlaying.visible ? nowPlaying.implicitHeight + Theme.spaceLg * 2 : 0
        visible: nowPlaying.visible
        color: Theme.surface
        Rectangle { width: parent.width; height: 1; color: Theme.divider }
    }
    DirectionalFocus {
        id: nowPlaying
        objectName: "homeNowPlaying"
        anchors {
            left: parent.left; right: parent.right; bottom: parent.bottom
            leftMargin: root.pageMargin; rightMargin: root.pageMargin; bottomMargin: Theme.spaceLg
        }
        implicitHeight: 90
        visible: root.playing
        Accessible.name: qsTranslate("Main", "Now playing")
        navigationItems: [returnButton, playbackButton, stopButton, muteButton]
        initialItem: returnButton
        onBoundaryReached: function(key) { if (key === Qt.Key_Up) root.leave(nowPlaying, -1, -1); }
        RowLayout {
            anchors.fill: parent
            spacing: Theme.spaceLg
            // The picture keeps running here; selecting it goes back to watching.
            Rectangle {
                objectName: "homePreview"
                Layout.preferredWidth: 160
                Layout.preferredHeight: 90
                color: Theme.canvas
                radius: Theme.controlRadius
                clip: true
                ShaderEffectSource {
                    anchors.fill: parent
                    visible: root.previewAvailable && root.previewSource !== null
                    sourceItem: visible ? root.previewSource : null
                    live: true
                }
                ChannelLogo {
                    anchors.centerIn: parent
                    width: 64; height: Theme.compactControlHeight
                    visible: !root.previewAvailable
                    logoUrl: root.playingLogo
                }
                MouseArea {
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    onClicked: root.returnRequested()
                }
            }
            ColumnLayout {
                Layout.fillWidth: true
                spacing: Theme.spaceXs
                Label {
                    objectName: "homeNowPlayingChannel"
                    Layout.fillWidth: true
                    text: root.playingChannel.replace(/^\d+\s+/, "")
                    color: Theme.textSecondary
                    font.pixelSize: Theme.fontCaption
                    textFormat: Text.PlainText
                    elide: Text.ElideRight
                }
                ChannelProgram {
                    objectName: "homeNowPlayingProgram"
                    readonly property var parsed: JSON.parse(root.playingProgramJson)
                    Layout.fillWidth: true
                    visible: parsed !== null
                    program: parsed
                    now: root.now
                }
                // Files and recordings without program data show their name instead.
                Label {
                    objectName: "homeNowPlayingTitle"
                    Layout.fillWidth: true
                    visible: root.playingProgramJson === "null"
                    text: root.playingTitle
                    color: Theme.textPrimary
                    font.pixelSize: Theme.fontControl
                    font.bold: true
                    textFormat: Text.PlainText
                    elide: Text.ElideRight
                }
            }
            ActionButton {
                id: returnButton
                objectName: "homeReturn"
                emphasis: ActionButton.Primary
                text: qsTranslate("Main", "Return to watching")
                onClicked: root.returnRequested()
            }
            IconAction {
                id: playbackButton
                objectName: "homePlayback"
                visible: root.actions !== null && root.actions.playbackToggle.enabled
                iconSource: root.iconDirectory + (root.actions ? root.actions.playbackIcon : "play.svg")
                tip: root.actions ? root.actions.playbackToggle.text : ""
                onClicked: root.actions.playbackToggle.trigger()
            }
            IconAction {
                id: stopButton
                objectName: "homeStop"
                visible: root.actions !== null && root.actions.separateStop
                iconSource: root.iconDirectory + "square.svg"
                tip: root.actions ? root.actions.stopPlayback.text : ""
                onClicked: root.actions.stopPlayback.trigger()
            }
            IconAction {
                id: muteButton
                objectName: "homeMute"
                visible: root.actions !== null
                iconSource: root.iconDirectory + (root.actions && root.actions.backend.audio_muted ? "volume-x.svg" : "volume-2.svg")
                tip: root.actions ? root.actions.toggleMute.text : ""
                onClicked: root.actions.toggleMute.trigger()
            }
        }
    }
}
