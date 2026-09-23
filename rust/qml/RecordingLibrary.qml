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
    color: Theme.canvas
    function clearSearch() { keywordField.clear(); }
    function search() {
        if (backend.epgstation_server.length && !backend.epgstation_busy)
            backend.browse_epgstation(backend.epgstation_server, keywordField.text);
    }
    function activate() {
        if (backend.epgstation_server.length && !backend.epgstation_loaded && !backend.epgstation_busy) search();
        keywordField.forceActiveFocus();
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
                objectName: "epgstationClose"
                flat: true
                iconSource: "qrc:/qt/qml/MinimalViewer/assets/icons/chevron-left.svg"
                tip: qsTranslate("RecordingLibrary", "Back to playback")
                onClicked: root.closeRequested()
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
        }
    }
    ColumnLayout {
        anchors {
            left: parent.left; right: parent.right; top: header.bottom; bottom: parent.bottom
            margins: Theme.spaceLg
        }
        spacing: Theme.spaceLg
        RowLayout {
            Layout.fillWidth: true
            spacing: Theme.spaceMd
            Label {
                Layout.fillWidth: true
                text: qsTranslate("RecordingLibrary", "EPGStation recordings")
                color: Theme.textPrimary
                font.pixelSize: Theme.fontHeading
            }
            ActionButton {
                objectName: "libraryOpenFile"
                text: qsTranslate("Recording", "Open TS file")
                onClicked: root.fileRequested()
            }
            ActionButton {
                objectName: "libraryOpenUrl"
                text: qsTranslate("Recording", "Open URL")
                onClicked: root.urlRequested()
            }
            ActionButton {
                objectName: "epgstationConnection"
                text: qsTranslate("RecordingLibrary", "Connection…")
                onClicked: root.connectionRequested()
            }
        }
        RowLayout {
            Layout.fillWidth: true
            spacing: Theme.spaceMd
            SettingsField {
                id: keywordField
                objectName: "epgstationKeyword"
                Layout.fillWidth: true
                maximumLength: 256
                placeholderText: qsTranslate("RecordingLibrary", "Search recordings")
                Accessible.name: placeholderText
                onAccepted: root.search()
            }
            ActionButton {
                objectName: "epgstationSearch"
                text: qsTranslate("RecordingLibrary", "Search")
                enabled: root.backend.epgstation_server.length > 0 && !root.backend.epgstation_busy
                onClicked: root.search()
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
                ScrollBar.vertical: ScrollBar {}
                delegate: Item {
                    id: row
                    required property string recordedId
                    required property string programName
                    required property string channelName
                    required property real startMs
                    required property real endMs
                    required property string description
                    required property bool playable
                    required property string unavailableReason
                    width: list.width
                    height: content.implicitHeight + Theme.spaceLg * 2
                    CardSurface { anchors.fill: parent; selected: false; hovered: false; pressed: false }
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
                                      : qsTranslate("RecordingLibrary", "No recorded TS file")
                                color: Theme.textSecondary
                                font.pixelSize: Theme.fontCaption
                                textFormat: Text.PlainText
                                elide: Text.ElideRight
                            }
                        }
                        ActionButton {
                            objectName: "epgstationPlay"
                            text: qsTranslate("RecordingLibrary", "Play")
                            enabled: row.playable && !root.backend.recording_loading
                            onClicked: root.backend.play_epgstation(row.recordedId)
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
        RowLayout {
            Layout.fillWidth: true
            spacing: Theme.spaceMd
            ActionButton {
                text: qsTranslate("RecordingLibrary", "Previous")
                enabled: root.backend.epgstation_previous
                onClicked: root.backend.epgstation_change_page(false)
            }
            Label {
                Layout.fillWidth: true
                text: qsTranslate("RecordingLibrary", "Page %1 · %2 recordings").arg(root.backend.epgstation_page).arg(root.backend.epgstation_total)
                horizontalAlignment: Text.AlignHCenter
                color: Theme.textSecondary
                font.pixelSize: Theme.fontCaption
            }
            ActionButton {
                text: qsTranslate("RecordingLibrary", "Next")
                enabled: root.backend.epgstation_next
                onClicked: root.backend.epgstation_change_page(true)
            }
        }
    }
}
