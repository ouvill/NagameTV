import QtQuick
import MinimalViewer

// What the recording server will do with a program: "", reserved, conflict,
// skipped (excluded from its rule), overlap or recording. Colors come from the guide palette so the mark reads on
// its light cells as well as on dark panels.
Rectangle {
    id: root
    required property string reservation
    readonly property bool problem: reservation === "conflict" || reservation === "skipped" || reservation === "overlap"
    visible: reservation.length > 0
    implicitWidth: label.implicitWidth + Theme.spaceSm * 2
    implicitHeight: label.implicitHeight + Theme.spaceXs
    radius: height / 2
    color: reservation === "recording" ? GuidePalette.recordingMark : problem ? GuidePalette.reservationProblemMark : GuidePalette.reservedMark
    Accessible.role: Accessible.StaticText
    Accessible.name: label.text
    Text {
        id: label
        anchors.centerIn: parent
        text: root.reservation === "recording" ? qsTranslate("Viewer", "Recording now")
            : root.reservation === "conflict" ? qsTranslate("Viewer", "Reservation conflict")
            : root.reservation === "skipped" ? qsTranslate("Viewer", "Excluded")
            : root.reservation === "overlap" ? qsTranslate("Viewer", "Duplicate reservation")
            : root.reservation === "reserved" ? qsTranslate("Viewer", "Reserved") : ""
        color: GuidePalette.markText
        font.pixelSize: Theme.fontMicro
        font.bold: true
    }
}
