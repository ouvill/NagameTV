pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import MinimalViewer

Popup {
    id: root
    property string logText: ""
    signal refreshRequested

    parent: Overlay.overlay
    anchors.centerIn: parent
    width: Math.min(900, parent.width - Theme.spaceXl * 2)
    height: Math.min(580, parent.height - Theme.spaceXl * 2)
    modal: true
    focus: true
    padding: Theme.spaceXl
    closePolicy: Popup.NoAutoClose
    background: PanelSurface {}
    enter: Transition { ScreenFade { entering: true; from: 0; to: 1 } }
    exit: Transition { ScreenFade { entering: false; from: 1; to: 0 } }

    function refresh() {
        refreshRequested();
        Qt.callLater(function() {
            if (root.visible) {
                logFlick.contentY = logFlick.end;
                logView.forceActiveFocus(Qt.TabFocusReason);
            }
        });
    }
    function back(event: KeyEvent) {
        event.accepted = true;
        if (!event.isAutoRepeat) close();
    }
    onOpened: refresh()
    onClosed: logText = ""

    contentItem: FocusScope {
        Keys.onEscapePressed: function(event) { root.back(event); }
        Keys.onBackPressed: function(event) { root.back(event); }
        ColumnLayout {
            anchors.fill: parent
            spacing: Theme.spaceMd
            Label {
                text: qsTranslate("Settings", "Recent logs")
                color: Theme.textPrimary
                font.pixelSize: Theme.fontHeading
                Layout.fillWidth: true
            }
            Control {
                id: logView
                objectName: "recentLogView"
                Layout.fillWidth: true
                Layout.fillHeight: true
                activeFocusOnTab: true
                focus: true
                padding: Theme.spaceSm
                background: ControlSurface { focused: logView.activeFocus }
                Keys.onUpPressed: logFlick.scroll(-1)
                Keys.onDownPressed: {
                    if (logFlick.atYEnd) refreshLogs.forceActiveFocus(Qt.TabFocusReason);
                    else logFlick.scroll(1);
                }
                contentItem: Flickable {
                    id: logFlick
                    objectName: "recentLogFlickable"
                    readonly property real end: Math.max(0, contentHeight - height)
                    clip: true
                    contentWidth: width
                    contentHeight: logBody.height
                    boundsBehavior: Flickable.StopAtBounds
                    ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }
                    function scroll(direction: int) {
                        contentY = Math.max(0, Math.min(end, contentY + direction * height * 0.75));
                    }
                    Text {
                        id: logBody
                        objectName: "recentLogText"
                        width: Math.max(0, logFlick.width - Theme.spaceLg)
                        text: root.logText || qsTranslate("Settings", "No log entries yet.")
                        textFormat: Text.PlainText
                        wrapMode: Text.Wrap
                        color: Theme.textSecondary
                        font.pixelSize: Theme.fontBody
                    }
                }
            }
            RowLayout {
                Layout.alignment: Qt.AlignRight
                spacing: Theme.spaceMd
                ActionButton {
                    id: refreshLogs
                    objectName: "refreshRecentLog"
                    text: qsTranslate("Settings", "Refresh logs")
                    onClicked: root.refresh()
                    KeyNavigation.up: logView
                    KeyNavigation.right: closeLogs
                }
                ActionButton {
                    id: closeLogs
                    objectName: "closeRecentLog"
                    text: qsTranslate("Main", "Close")
                    onClicked: root.close()
                    KeyNavigation.up: logView
                    KeyNavigation.left: refreshLogs
                }
            }
        }
    }
}
