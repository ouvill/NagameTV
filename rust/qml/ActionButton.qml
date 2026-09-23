pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import MinimalViewer

Button {
    id: control
    enum Emphasis { Secondary, Primary, Quiet }
    enum Surface { Standard, VideoOverlay }
    property int emphasis: ActionButton.Secondary
    property int surface: ActionButton.Standard
    property bool selected: false
    property int textAlignment: Text.AlignHCenter
    property bool wrapText: false
    property real feedbackScale: down ? Theme.pressScale : 1
    implicitWidth: contentItem.implicitWidth + leftPadding + rightPadding
    implicitHeight: Math.max(Theme.controlHeight, contentItem.implicitHeight + topPadding + bottomPadding)
    leftPadding: Theme.spaceLg
    rightPadding: Theme.spaceLg
    topPadding: Theme.spaceSm
    bottomPadding: Theme.spaceSm
    hoverEnabled: true
    opacity: enabled ? 1 : Theme.disabledOpacity
    Behavior on feedbackScale {
        NumberAnimation { duration: control.down ? Theme.pressDuration : Theme.moveDuration; easing.type: Easing.OutCubic }
    }
    contentItem: Label {
        scale: control.feedbackScale
        text: control.text
        textFormat: Text.PlainText
        color: control.emphasis === ActionButton.Primary ? Theme.textOnAccent
            : control.highlighted ? Theme.accent
            : control.emphasis === ActionButton.Quiet ? Theme.textSecondary : Theme.textPrimary
        font.pixelSize: Theme.fontBody
        font.bold: control.emphasis === ActionButton.Primary
        verticalAlignment: Text.AlignVCenter
        horizontalAlignment: control.textAlignment
        wrapMode: control.wrapText ? Text.Wrap : Text.NoWrap
        elide: control.wrapText ? Text.ElideNone : Text.ElideRight
    }
    background: Rectangle {
        scale: control.feedbackScale
        radius: control.surface === ActionButton.VideoOverlay ? height / 2 : Theme.controlRadius
        color: control.emphasis === ActionButton.Primary
            ? (control.down ? Theme.accentPressed : control.hovered ? Theme.accentHover : Theme.accent)
            : control.down ? (control.surface === ActionButton.VideoOverlay ? Theme.overlayPressed : Theme.surfacePressed)
            : control.selected ? Theme.selection
            : control.hovered ? (control.surface === ActionButton.VideoOverlay ? Theme.overlayHover : Theme.surfaceHover)
            : control.emphasis === ActionButton.Quiet || control.surface === ActionButton.VideoOverlay
                ? "transparent" : Theme.surfaceRaised
        border.color: control.visualFocus || control.selected ? Theme.accent : "transparent"
        Behavior on color { ColorAnimation { duration: Theme.colorDuration } }
        Behavior on border.color { ColorAnimation { duration: Theme.colorDuration } }
    }
}
