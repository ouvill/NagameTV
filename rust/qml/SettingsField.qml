pragma ComponentBehavior: Bound
import QtQuick
import MinimalViewer
import QtQuick.Controls

TextField {
    id: control
    implicitHeight: Theme.controlHeight
    opacity: enabled ? 1 : Theme.disabledOpacity
    color: Theme.textPrimary
    placeholderTextColor: Theme.textMuted
    selectionColor: Theme.accent
    selectedTextColor: Theme.textOnAccent
    font.pixelSize: Theme.fontControl
    selectByMouse: true
    leftPadding: Theme.spaceLg
    rightPadding: Theme.spaceLg
    background: ControlSurface {
        focused: control.activeFocus
        hovered: control.hovered
    }
}
