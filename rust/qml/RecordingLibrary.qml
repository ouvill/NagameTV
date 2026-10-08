pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import MinimalViewer

Rectangle {
    id: root
    objectName: "recordingLibrary"
    required property Player backend
    required property Window targetWindow
    signal closeRequested
    signal modeRequested(int mode)
    signal fileRequested
    signal urlRequested
    signal connectionRequested
    enum AfterLoad { KeepFocus, ResetSearch, FocusFirstRecording }
    property int afterLoad: RecordingLibrary.KeepFocus
    color: Theme.canvas
    function clearSearch() { keywordField.clear(); }
    function search() {
        if (backend.epgstation_server.length && !backend.epgstation_busy) {
            afterLoad = RecordingLibrary.ResetSearch;
            if (!backend.browse_epgstation(backend.epgstation_server, keywordField.text)) afterLoad = RecordingLibrary.KeepFocus;
        }
    }
    function activate() {
        if (backend.epgstation_server.length && !backend.epgstation_loaded && !backend.epgstation_busy) search();
        keywordField.focusForNavigation();
    }
    function focusRecordings(last: bool) {
        if (list.count > 0) {
            if (last) list.currentIndex = list.count - 1;
            else if (list.currentIndex < 0) list.currentIndex = 0;
            list.positionViewAtIndex(list.currentIndex, ListView.Contain);
            list.forceActiveFocus(Qt.TabFocusReason);
        } else keywordField.focusForNavigation();
    }
    function focusPages() {
        if (previousPage.enabled || nextPage.enabled) pagination.enter();
    }
    function moveRecording(offset: int) {
        if (offset < 0 && list.currentIndex <= 0) keywordField.focusForNavigation();
        else if (offset > 0 && list.currentIndex + 1 >= list.count) focusPages();
        else {
            list.currentIndex += offset;
            list.positionViewAtIndex(list.currentIndex, ListView.Contain);
        }
    }
    function chooseCurrent(event) {
        if (event.isAutoRepeat) return;
        const row = list.currentItem as RecordingRow;
        if (row) row.choose();
    }
    function changePage(forward: bool) {
        afterLoad = pagination.navigating ? RecordingLibrary.FocusFirstRecording : RecordingLibrary.KeepFocus;
        if (afterLoad === RecordingLibrary.FocusFirstRecording) keywordField.focusForNavigation();
        root.backend.epgstation_change_page(forward);
    }
    Connections {
        target: root.backend
        function onEpgstation_busyChanged() {
            if (root.backend.epgstation_busy || root.afterLoad === RecordingLibrary.KeepFocus) return;
            const action = root.afterLoad;
            root.afterLoad = RecordingLibrary.KeepFocus;
            Qt.callLater(function() {
                if (root.backend.epgstation_error.length) return;
                if (action === RecordingLibrary.ResetSearch) {
                    list.currentIndex = list.count > 0 ? 0 : -1;
                    list.positionViewAtBeginning();
                } else if (root.visible && keywordField.activeFocus && keywordField.interaction === NavigationField.Navigating) {
                    list.currentIndex = list.count > 0 ? 0 : -1;
                    root.focusRecordings(false);
                }
            });
        }
    }
    RecordingFiles {
        id: filesDialog
        files: root.backend.recording_files
        onFileChosen: function(recordedId, videoId) { root.backend.play_epgstation_file(recordedId, videoId); }
    }
    Item {
        id: header
        anchors { left: parent.left; right: parent.right; top: parent.top }
        height: navigation.headerHeight
        WindowDragArea { anchors.fill: parent; targetWindow: root.targetWindow }
        Row {
            anchors { left: parent.left; leftMargin: navigation.edgeMargin; verticalCenter: parent.verticalCenter }
            spacing: Theme.spaceMd
            IconAction {
                id: closeLibrary
                objectName: "epgstationClose"
                flat: true
                iconSource: "qrc:/qt/qml/MinimalViewer/assets/icons/chevron-left.svg"
                tip: qsTranslate("RecordingLibrary", "Back to playback")
                onClicked: root.closeRequested()
                Keys.onRightPressed: navigation.enter()
                Keys.onDownPressed: actions.enter()
            }
            Label {
                anchors.verticalCenter: parent.verticalCenter
                text: qsTranslate("RecordingLibrary", "Recordings")
                color: Theme.textPrimary
                font.pixelSize: Theme.fontTitle
                font.bold: true
            }
        }
        ModeNavigation {
            id: navigation
            objectName: "recordingModeNavigation"
            targetWindow: root.targetWindow
            mode: ModeNavigation.Recording
            guideEnabled: root.backend.epg_enabled
            onModeRequested: function(mode) { root.modeRequested(mode); }
            onBoundaryReached: function(key) {
                if (key === Qt.Key_Down) actions.enter();
                else if (key === Qt.Key_Left) closeLibrary.forceActiveFocus(Qt.TabFocusReason);
            }
        }
    }
    ColumnLayout {
        anchors {
            left: parent.left; right: parent.right; top: header.bottom; bottom: parent.bottom
            margins: Theme.spaceLg
        }
        spacing: Theme.spaceLg
        DirectionalFocus {
            id: actions
            Layout.fillWidth: true
            implicitHeight: actionRow.implicitHeight
            navigationItems: [openFile, openUrl, connection]
            onBoundaryReached: function(key) {
                if (key === Qt.Key_Down) keywordField.focusForNavigation();
                else if (key === Qt.Key_Up) navigation.enter();
                else if (key === Qt.Key_Left) closeLibrary.forceActiveFocus(Qt.TabFocusReason);
            }
            RowLayout {
                id: actionRow
                anchors.fill: parent
                spacing: Theme.spaceMd
                Label {
                    Layout.fillWidth: true
                    text: qsTranslate("RecordingLibrary", "EPGStation recordings")
                    color: Theme.textPrimary
                    font.pixelSize: Theme.fontHeading
                }
                ActionButton {
                    id: openFile
                    objectName: "libraryOpenFile"
                    text: qsTranslate("Recording", "Open video file")
                    onClicked: root.fileRequested()
                }
                ActionButton {
                    id: openUrl
                    objectName: "libraryOpenUrl"
                    text: qsTranslate("Recording", "Open URL")
                    onClicked: root.urlRequested()
                }
                ActionButton {
                    id: connection
                    objectName: "epgstationConnection"
                    text: qsTranslate("RecordingLibrary", "Connection…")
                    onClicked: root.connectionRequested()
                }
            }
        }
        RowLayout {
            Layout.fillWidth: true
            spacing: Theme.spaceMd
            NavigationField {
                id: keywordField
                objectName: "epgstationKeyword"
                Layout.fillWidth: true
                maximumLength: 256
                placeholderText: qsTranslate("RecordingLibrary", "Search recordings")
                Accessible.name: placeholderText
                onAccepted: { finishEditing(); root.search(); }
                onNavigationRequested: function(key) {
                    if (key === Qt.Key_Up) actions.enter();
                    else if (key === Qt.Key_Down) {
                        if (list.count > 0) root.focusRecordings(false);
                        else root.focusPages();
                    } else if (key === Qt.Key_Right && searchButton.enabled) searchButton.forceActiveFocus(Qt.TabFocusReason);
                }
            }
            ActionButton {
                id: searchButton
                objectName: "epgstationSearch"
                text: qsTranslate("RecordingLibrary", "Search")
                enabled: root.backend.epgstation_server.length > 0 && !root.backend.epgstation_busy
                onClicked: root.search()
                Keys.onLeftPressed: keywordField.focusForNavigation()
                Keys.onUpPressed: actions.enter()
                Keys.onDownPressed: root.focusRecordings(false)
                onEnabledChanged: if (!enabled && activeFocus) keywordField.focusForNavigation()
            }
        }
        Label {
            Layout.fillWidth: true
            visible: text.length > 0
            text: root.backend.epgstation_error
            color: Theme.error
            font.pixelSize: Theme.fontCaption
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
        }
        Label {
            Layout.fillWidth: true
            visible: text.length > 0
            text: root.backend.settings_error
            color: Theme.error
            font.pixelSize: Theme.fontCaption
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
        }
        Item {
            Layout.fillWidth: true
            Layout.fillHeight: true
            ListView {
                id: list
                objectName: "epgstationRecordings"
                anchors.fill: parent
                clip: true
                spacing: Theme.spaceMd
                model: root.backend.recordings
                keyNavigationEnabled: false
                activeFocusOnTab: true
                Keys.onUpPressed: root.moveRecording(-1)
                Keys.onDownPressed: root.moveRecording(1)
                Keys.onReturnPressed: function(event) { root.chooseCurrent(event); }
                Keys.onEnterPressed: function(event) { root.chooseCurrent(event); }
                onCountChanged: if (count === 0 && activeFocus) keywordField.focusForNavigation()
                ScrollBar.vertical: ScrollBar {}
                delegate: RecordingRow {}
                component RecordingRow: Item {
                    id: row
                    objectName: "epgstationRecording-" + row.recordedId
                    required property int index
                    required property string recordedId
                    required property string programName
                    required property string channelName
                    required property real startMs
                    required property real endMs
                    required property string description
                    required property int fileCount
                    required property bool playable
                    required property string unavailableReason
                    width: list.width
                    height: content.implicitHeight + Theme.spaceLg * 2
                    function choose() {
                        if (!playButton.enabled) return;
                        list.currentIndex = index;
                        if (row.fileCount === 1) root.backend.play_epgstation(row.recordedId);
                        else if (root.backend.choose_epgstation(row.recordedId)) {
                            filesDialog.recordedId = row.recordedId;
                            filesDialog.open();
                        }
                    }
                    CardSurface {
                        anchors.fill: parent
                        selected: row.ListView.isCurrentItem && list.activeFocus
                        hovered: rowPointer.containsMouse || playButton.hovered
                        pressed: rowPointer.pressed || playButton.down
                    }
                    MouseArea {
                        id: rowPointer
                        anchors.fill: parent
                        enabled: playButton.enabled
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: {
                            list.forceActiveFocus(Qt.MouseFocusReason);
                            row.choose();
                        }
                    }
                    RowLayout {
                        id: content
                        anchors { left: parent.left; right: parent.right; top: parent.top; margins: Theme.spaceLg }
                        spacing: Theme.spaceLg
                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: Theme.spaceXs
                            Label {
                                Layout.fillWidth: true
                                text: row.programName
                                color: Theme.textPrimary
                                font.pixelSize: Theme.fontBody
                                textFormat: Text.PlainText
                                wrapMode: Text.Wrap
                                maximumLineCount: 2
                                elide: Text.ElideRight
                            }
                            Label {
                                Layout.fillWidth: true
                                text: row.channelName + "  " + new Date(row.startMs).toLocaleString(Qt.locale(root.backend.ui_language), "yyyy/MM/dd HH:mm")
                                      + " – " + new Date(row.endMs).toLocaleTimeString(Qt.locale(root.backend.ui_language), "HH:mm")
                                color: Theme.textSecondary
                                textFormat: Text.PlainText
                                font.pixelSize: Theme.fontCaption
                                elide: Text.ElideRight
                            }
                            Label {
                                Layout.fillWidth: true
                                visible: text.length > 0
                                text: row.playable ? row.description : row.unavailableReason === "recording"
                                      ? qsTranslate("RecordingLibrary", "Recording in progress")
                                      : qsTranslate("RecordingLibrary", "No recorded video file")
                                color: Theme.textSecondary
                                font.pixelSize: Theme.fontCaption
                                textFormat: Text.PlainText
                                elide: Text.ElideRight
                            }
                        }
                        ActionButton {
                            id: playButton
                            objectName: "epgstationPlay"
                            text: qsTranslate("RecordingLibrary", "Play")
                            enabled: row.playable && !root.backend.recording_loading
                            onClicked: row.choose()
                            Keys.onUpPressed: { list.currentIndex = row.index; list.forceActiveFocus(Qt.TabFocusReason); root.moveRecording(-1); }
                            Keys.onDownPressed: { list.currentIndex = row.index; list.forceActiveFocus(Qt.TabFocusReason); root.moveRecording(1); }
                        }
                    }
                }
            }
            Label {
                anchors.centerIn: parent
                width: parent.width
                visible: !root.backend.epgstation_busy && list.count === 0
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.Wrap
                text: root.backend.epgstation_loaded ? qsTranslate("RecordingLibrary", "No recordings found")
                      : qsTranslate("RecordingLibrary", "Set up EPGStation in Settings → Connection to browse recordings.")
                color: Theme.textSecondary
                font.pixelSize: Theme.fontBody
            }
            BusyIndicator {
                anchors.centerIn: parent
                running: root.backend.epgstation_busy
                visible: running
            }
        }
        DirectionalFocus {
            id: pagination
            Layout.fillWidth: true
            implicitHeight: pageRow.implicitHeight
            navigationItems: [previousPage, nextPage]
            onBoundaryReached: function(key) { if (key === Qt.Key_Up) root.focusRecordings(true); }
            RowLayout {
                id: pageRow
                anchors.fill: parent
                spacing: Theme.spaceMd
                ActionButton {
                    id: previousPage
                    objectName: "epgstationPrevious"
                    text: qsTranslate("RecordingLibrary", "Previous")
                    enabled: root.backend.epgstation_previous
                    onClicked: root.changePage(false)
                }
                Label {
                    Layout.fillWidth: true
                    text: qsTranslate("RecordingLibrary", "Page %1 · %2 recordings").arg(root.backend.epgstation_page).arg(root.backend.epgstation_total)
                    horizontalAlignment: Text.AlignHCenter
                    color: Theme.textSecondary
                    font.pixelSize: Theme.fontCaption
                }
                ActionButton {
                    id: nextPage
                    objectName: "epgstationNext"
                    text: qsTranslate("RecordingLibrary", "Next")
                    enabled: root.backend.epgstation_next
                    onClicked: root.changePage(true)
                }
            }
        }
    }
}
