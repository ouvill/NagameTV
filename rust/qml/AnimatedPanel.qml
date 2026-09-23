import QtQuick
import MinimalViewer

// The visible lifetime owns the loaded subtree, including interrupted closes.
Loader {
    id: panel
    enum Motion { Slide, Fade }
    property int motion: AnimatedPanel.Slide
    property bool open: false
    property bool shuttingDown: false
    active: !shuttingDown && (open || opacity > 0)
    visible: active
    enabled: open && !shuttingDown
    focus: enabled
    opacity: open && !shuttingDown ? 1 : 0
    Behavior on opacity {
        enabled: !panel.shuttingDown
        ScreenFade {
            entering: panel.open
            duration: panel.motion === AnimatedPanel.Fade
                ? (entering ? enterDurationMs : exitDurationMs) : Theme.moveDuration
            easing.type: panel.motion === AnimatedPanel.Fade ? Easing.Linear : Easing.OutCubic
        }
    }
    transform: Translate {
        property real slideY: panel.open && !panel.shuttingDown ? 0 : panel.height
        y: panel.motion === AnimatedPanel.Fade ? 0 : slideY
        Behavior on slideY {
            enabled: !panel.shuttingDown && panel.motion === AnimatedPanel.Slide
            NumberAnimation {
                duration: Theme.panelDuration
                easing.type: Easing.OutCubic
            }
        }
    }
}
