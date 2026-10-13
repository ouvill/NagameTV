pragma ComponentBehavior: Bound
import QtQuick
import MinimalViewer
import QtQuick.Controls
import QtQuick.Layouts

Popup {
    id: popup
    required property GuideModel guideModel
    required property var selection
    property int candidateIndex: 0
    readonly property GuideCandidates candidates: guideModel.candidates
    readonly property var program: {
        guideModel.revision;
        return selection ? guideModel.details(selection.watchKey, candidateIndex) : null;
    }
    function refreshCandidates() {
        candidateIndex = 0
        guideModel.select_candidates(selection ? selection.watchKey : "")
    }
    onSelectionChanged: refreshCandidates()
    Connections { target: popup.guideModel; function onChanged() { popup.refreshCandidates(); } }
    required property point cellPosition
    required property real channelWidth
    required property string channelLabel
    property url iconDirectory: "qrc:/qt/qml/MinimalViewer/assets/icons/"
    property string uiLanguage: Qt.uiLanguage
    // Backend translation source; retranslate even while the error remains visible.
    property string watchError: ""
    property double now: Date.now()
    readonly property int watchAction: {
        guideModel.revision;
        return selection ? guideModel.action(selection.watchKey, now) : GuideModel.Unavailable;
    }
    readonly property bool live: watchAction !== GuideModel.Unavailable
    signal watchRequested(string key)
    // Recording reservations; the owner reads these from the backend for `selection`.
    property bool reservationsEnabled: false
    property bool reservationBusy: false
    property string reservation: ""
    property bool canReserve: false
    property bool canCancelReservation: false
    // Backend translation source, like watchError.
    property string reservationError: ""
    signal reserveRequested(string key)
    signal cancelReservationRequested(string key)

    // Keep coordinates, not a delegate reference: offscreen columns are destroyed.
    width: Math.max(0, Math.min(560, parent.width - 48))
    height: Math.max(0, Math.min(620, parent.height - 40))
    x: Math.max(24, Math.min(parent.width - width - 24,
        cellPosition.x > parent.width / 2 ? cellPosition.x - width - 28 : cellPosition.x + channelWidth + 24))
    y: Math.max(20, Math.min(parent.height - height - 20, cellPosition.y))
    padding: Theme.spaceXl
    margins: 0
    modal: false
    dim: false
    focus: true
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
    transformOrigin: Popup.Center
    enter: Transition {
        NumberAnimation { property: "opacity"; from: 0; to: 1; duration: Theme.moveDuration; easing.type: Easing.OutCubic }
        NumberAnimation { property: "scale"; from: 0.97; to: 1; duration: Theme.moveDuration; easing.type: Easing.OutBack }
    }
    exit: Transition {
        NumberAnimation { property: "opacity"; to: 0; duration: Theme.moveDuration; easing.type: Easing.OutCubic }
        NumberAnimation { property: "scale"; to: 0.97; duration: Theme.moveDuration; easing.type: Easing.OutBack }
    }
    Component.onCompleted: { refreshCandidates(); open(); }
    onAboutToShow: now = Date.now()
    Timer { interval: 1000; repeat: true; running: popup.opened; onTriggered: popup.now = Date.now() }
    background: PanelSurface {}
    contentItem: ColumnLayout {
        spacing: Theme.spaceLg
        RowLayout {
            Layout.fillWidth: true
            Label {
                text: qsTranslate("Viewer", "Program details")
                color: Theme.textSecondary; font.pixelSize: Theme.fontCaption
                Layout.fillWidth: true
            }
            IconAction {
                objectName: "closeGuideProgramDetails"
                Layout.preferredWidth: Theme.compactControlHeight
                Layout.preferredHeight: Theme.compactControlHeight
                iconSource: popup.iconDirectory + "x.svg"
                tip: qsTranslate("Main", "Close")
                iconSize: Theme.smallIconSize
                flat: true
                onClicked: popup.close()
            }
        }
        ScrollView {
            id: detailsScroll
            Layout.fillWidth: true
            Layout.fillHeight: true
            rightPadding: Theme.spaceMd
            clip: true
            focus: true
            ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
            ScrollBar.vertical.policy: detailsFlick.contentHeight > detailsFlick.height
                ? ScrollBar.AlwaysOn : ScrollBar.AlwaysOff
            ScrollBar.vertical.palette.mid: Theme.textMuted
            contentItem: Flickable {
                id: detailsFlick
                objectName: "programDetailsFlickable"
                contentWidth: width
                contentHeight: body.implicitHeight
                boundsBehavior: Flickable.StopAtBounds
                flickableDirection: Flickable.VerticalFlick
                Column {
                    id: body
                    width: detailsFlick.width
                    spacing: Theme.spaceLg
                    Label {
                        width: parent.width
                        visible: popup.selection && popup.selection.scheduleState === GuideModel.Conflict
                        text: qsTranslate("Viewer", "Program information overlaps. The current program cannot be determined from the schedule.")
                        textFormat: Text.PlainText; wrapMode: Text.Wrap
                        color: Theme.textSecondary; font.pixelSize: Theme.fontCaption
                    }
                    SettingsChoice {
                        objectName: "guideCandidateSelector"
                        width: parent.width
                        visible: popup.candidates.count > 1
                        model: popup.candidates
                        textRole: "name"
                        currentIndex: popup.candidateIndex
                        onActivated: function(index) { popup.candidateIndex = index }
                    }
                    Label {
                        objectName: "programDateTime"
                        width: parent.width
                        text: {
                            if (!popup.program) return ""
                            const start = new Date(popup.program.startAt)
                            const end = new Date(popup.program.startAt + popup.program.duration)
                            const format = qsTranslate("Viewer", "ddd, MMM d · hh:mm")
                            const locale = Qt.locale(popup.uiLanguage)
                            if (popup.program.endUnknown) return start.toLocaleString(locale, format) + " – " + qsTranslate("Viewer", "End time unknown")
                            return start.toLocaleString(locale, format) + " – "
                                + end.toLocaleString(locale, start.toDateString() === end.toDateString() ? "hh:mm" : format)
                        }
                        color: Theme.textSecondary; font.pixelSize: Theme.fontCaption
                        wrapMode: Text.Wrap
                    }
                    Label {
                        objectName: "programTitle"
                        width: parent.width
                        text: popup.program ? (popup.program.name || qsTranslate("Viewer", "No program information")) : ""
                        textFormat: Text.PlainText
                        color: Theme.textPrimary; font.pixelSize: Theme.fontTitle; font.bold: true
                        wrapMode: Text.Wrap; lineHeight: 1.15
                    }
                    Label {
                        objectName: "programChannel"
                        width: parent.width
                        text: popup.channelLabel; textFormat: Text.PlainText
                        color: Theme.textSecondary; wrapMode: Text.Wrap
                    }
                    ReservationBadge {
                        objectName: "programReservation"
                        reservation: popup.reservationsEnabled ? popup.reservation : ""
                    }
                    ProgramFacts {
                        objectName: "programFacts"
                        width: parent.width
                        program: popup.program
                        now: popup.now
                    }
                    Rectangle { width: parent.width; height: 1; color: Theme.overlayBorder }
                    Label {
                        objectName: "programDescription"
                        width: parent.width
                        text: popup.program ? (popup.program.description || "") : ""
                        textFormat: Text.PlainText; color: Theme.textPrimary
                        visible: text.length > 0
                        wrapMode: Text.Wrap; font.pixelSize: Theme.fontBody; lineHeight: 1.25
                    }
                    ProgramMetadata {
                        objectName: "programMetadata"
                        width: parent.width
                        program: popup.program
                        visible: implicitHeight > 0
                    }
                    Label {
                        objectName: "reservationGuideError"
                        visible: popup.reservationsEnabled && popup.reservationError !== ""
                        width: parent.width
                        text: popup.reservationError ? qsTranslate("Backend", popup.reservationError) : ""; textFormat: Text.PlainText
                        wrapMode: Text.Wrap; color: Theme.error
                        onTextChanged: function(text) { if (text) revealError.restart() }
                    }
                    Label {
                        objectName: "watchGuideError"
                        visible: popup.watchError !== ""
                        width: parent.width
                        text: popup.watchError ? qsTranslate("Backend", popup.watchError) : ""; textFormat: Text.PlainText
                        wrapMode: Text.Wrap; color: Theme.error
                        onTextChanged: function(text) { if (text) revealError.restart() }
                    }
                }
                // Wait for wrapped error text to settle before revealing its last line.
                Timer {
                    id: revealError
                    interval: 0
                    onTriggered: {
                        body.forceLayout()
                        detailsFlick.contentY = Math.max(0, detailsFlick.contentHeight - detailsFlick.height)
                    }
                }
            }
        }
        DirectionalFocus {
            id: actions
            objectName: "guideProgramActions"
            Layout.fillWidth: true
            implicitHeight: actionRow.implicitHeight
            // From the conditions, not the buttons: a hidden row hides its buttons too.
            visible: popup.live || (popup.reservationsEnabled && (popup.canReserve || popup.canCancelReservation))
            navigationItems: [watchButton, reserveButton, cancelReservationButton]
            RowLayout {
                id: actionRow
                anchors.fill: parent
                spacing: Theme.spaceMd
                ActionButton {
                    emphasis: ActionButton.Primary
                    id: watchButton
                    objectName: "watchGuideProgram"
                    visible: popup.live
                    Layout.preferredWidth: Math.max(168, implicitWidth)
                    Layout.preferredHeight: Theme.controlHeight
                    text: popup.watchAction === GuideModel.WatchChannel ? qsTranslate("Viewer", "Watch this channel") : qsTranslate("Viewer", "Watch this program")
                    onClicked: popup.watchRequested(popup.selection.watchKey)
                }
                ActionButton {
                    id: reserveButton
                    objectName: "reserveGuideProgram"
                    // A program already on air is watched first; one still ahead is reserved first.
                    emphasis: popup.live ? ActionButton.Secondary : ActionButton.Primary
                    visible: popup.reservationsEnabled && popup.canReserve
                    enabled: !popup.reservationBusy
                    Layout.preferredHeight: Theme.controlHeight
                    text: qsTranslate("Viewer", "Reserve recording")
                    onClicked: popup.reserveRequested(popup.selection.watchKey)
                }
                ActionButton {
                    id: cancelReservationButton
                    objectName: "cancelGuideReservation"
                    visible: popup.reservationsEnabled && popup.canCancelReservation
                    enabled: !popup.reservationBusy
                    Layout.preferredHeight: Theme.controlHeight
                    text: qsTranslate("Viewer", "Cancel reservation")
                    onClicked: popup.cancelReservationRequested(popup.selection.watchKey)
                }
                BusyIndicator {
                    objectName: "reservationBusy"
                    implicitWidth: Theme.compactControlHeight
                    implicitHeight: Theme.compactControlHeight
                    visible: popup.reservationsEnabled && popup.reservationBusy
                    running: visible
                    palette.dark: Theme.accent
                    palette.text: Theme.accent
                    Accessible.name: qsTranslate("Viewer", "Loading…")
                }
                Item { Layout.fillWidth: true }
            }
        }
    }
}
