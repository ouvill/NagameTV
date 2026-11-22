pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import MinimalViewer 1.0 as Native

Item {
    id: root
    required property ViewerActions actions
    required property InputContext inputContext
    Native.ShortcutKey { id: keyMatcher }
    Item {
        id: backButtonBinding
        enabled: root.actions.enabled && root.inputContext.enabled
        readonly property bool popupOpen: root.inputContext.popupOpen
        Component.onCompleted: keyMatcher.bind_back_button(backButtonBinding, popupOpen)
        onPopupOpenChanged: keyMatcher.bind_back_button(backButtonBinding, popupOpen)
    }
    // Linux IBus can forward keys directly to the focus object, bypassing
    // Qt's window shortcut map. The native key normalizer also uses focus
    // delivery for remapped presses. Ordinary shortcuts are consumed before Keys.pressed.
    Connections {
        target: root.inputContext.focusItem ? root.inputContext.focusItem.Keys : null
        function onPressed(event) {
            if (!root.inputContext.targetWindow.active) return;
            event.accepted = root.dispatch(event.key, event.modifiers, event.isAutoRepeat);
        }
    }
    function dispatch(key: int, modifiers: int, autoRepeat: bool): bool {
        const matches = [...root.entries, ...root.digits, ...root.colours].filter(binding => binding.enabled
            && keyMatcher.matches(binding.portableText, key, modifiers));
        if (matches.length === 0) return false;
        if (matches.length > 1) {
            console.warn("Ambiguous shortcut: " + matches[0].portableText);
            return true;
        }
        const binding = matches[0];
        if (!autoRepeat || binding.autoRepeat) binding.operation.trigger();
        return true;
    }
    component Binding: ShortcutBinding {
        inputContext: root.inputContext
        active: root.enabled
    }
    // Station buttons 1–12 are typed as digits. They follow the same two
    // delivery paths as the bindings below but are not individual settings rows.
    property bool digitsAvailable: false
    signal digitPressed(int digit)
    component Digit: Binding {
        id: digit
        required property int value
        sequence: String(value)
        scope: InputContext.Navigation
        available: root.digitsAvailable
        operation: Action {
            enabled: root.actions.enabled
            onTriggered: root.digitPressed(digit.value)
        }
    }
    readonly property list<ShortcutBinding> digits: [
        Digit { value: 0 }, Digit { value: 1 }, Digit { value: 2 }, Digit { value: 3 }, Digit { value: 4 },
        Digit { value: 5 }, Digit { value: 6 }, Digit { value: 7 }, Digit { value: 8 }, Digit { value: 9 }
    ]
    // Dedicated colour keys stay available while BML presents, even when
    // focus is on a control other than the video.
    property bool coloursAvailable: false
    signal colourPressed(string key)
    component Colour: Binding {
        id: colour
        required property string domKey
        scope: InputContext.Window
        available: root.coloursAvailable
        operation: Action {
            enabled: root.actions.enabled
            onTriggered: root.colourPressed(colour.domKey)
        }
    }
    readonly property list<ShortcutBinding> colours: [
        Colour { sequence: "Red"; domKey: "r" },
        Colour { sequence: "Green"; domKey: "g" },
        Colour { sequence: "Yellow"; domKey: "y" },
        Colour { sequence: "Blue"; domKey: "b" }
    ]
    // One declaration creates both the registered shortcut and its settings row.
    readonly property list<ShortcutBinding> entries: [
        Binding { id: fullscreen; objectName: "fullscreenShortcut"; sequence: "F11"; operation: root.actions.toggleFullscreen; scope: InputContext.Window },
        Binding { id: recording; objectName: "recordingShortcut"; sequence: "Ctrl+O"; operation: root.actions.openRecording; scope: InputContext.Window },
        Binding { id: channels; objectName: "channelsShortcut"; sequence: "S"; operation: root.actions.toggleChannels; scope: InputContext.Navigation },
        Binding { id: composer; objectName: "composerShortcut"; sequence: "C"; operation: root.actions.openComposer; scope: InputContext.Navigation },
        Binding { id: guide; objectName: "guideShortcut"; sequence: "G"; operation: root.actions.toggleGuide; scope: InputContext.Navigation },
        Binding {
            id: screenshot
            objectName: "screenshotShortcut"
            sequence: "Ctrl+S"
            operation: root.actions.captureScreenshot
            scope: InputContext.Navigation
            available: root.inputContext.viewing
        },
        // A recording with chapters has no channel to step; the same keys move between chapters.
        Binding { id: previous; objectName: "previousChannelShortcut"; sequence: "PgUp"; operation: root.actions.previousChannel; scope: InputContext.Navigation; available: !root.actions.chaptersAvailable },
        Binding { id: next; objectName: "nextChannelShortcut"; sequence: "PgDown"; operation: root.actions.nextChannel; scope: InputContext.Navigation; available: !root.actions.chaptersAvailable },
        Binding {
            objectName: "channelUpShortcut"
            sequence: "Channel Up"
            operation: root.actions.nextChannel
            scope: InputContext.Navigation
            available: root.actions.liveViewing && root.inputContext.viewing
        },
        Binding {
            objectName: "channelDownShortcut"
            sequence: "Channel Down"
            operation: root.actions.previousChannel
            scope: InputContext.Navigation
            available: root.actions.liveViewing && root.inputContext.viewing
        },
        Binding { id: last; objectName: "lastChannelShortcut"; sequence: "L"; operation: root.actions.lastChannel; scope: InputContext.Navigation },
        Binding {
            id: playback
            objectName: "playbackShortcut"
            sequence: "Space"
            operation: root.actions.playbackToggle
            scope: InputContext.Playback
            description: qsTranslate("Settings", "Play or pause")
            condition: qsTranslate("Settings", "Recordings and pausable live TV")
        },
        // Desktop global grabs deliver through MPRIS. Keep these window-scoped
        // for keys that reach Qt. MPRIS calls carry no key identity, so a timing
        // heuristic could swallow a separate desktop request or a seek repeat.
        Binding {
            objectName: "playMediaShortcut"
            sequence: "Media Play"
            operation: root.actions.play
            scope: InputContext.Playback
            condition: playback.condition
        },
        Binding {
            objectName: "pauseMediaShortcut"
            sequence: "Media Pause"
            operation: root.actions.pause
            scope: InputContext.Playback
            condition: playback.condition
        },
        Binding {
            id: backward
            objectName: "seekBackwardShortcut"
            sequence: "Left"
            operation: root.actions.seekBackward
            scope: InputContext.Seek
            autoRepeat: true
            condition: qsTranslate("Settings", "Recordings and timeshift")
        },
        Binding {
            id: forward
            objectName: "seekForwardShortcut"
            sequence: "Right"
            operation: root.actions.seekForward
            scope: InputContext.Seek
            autoRepeat: true
            condition: qsTranslate("Settings", "Recordings and timeshift")
        },
        Binding {
            objectName: "rewindMediaShortcut"
            sequence: "Media Rewind"
            operation: root.actions.seekBackward
            scope: InputContext.Seek
            autoRepeat: backward.autoRepeat
            condition: backward.condition
        },
        Binding {
            objectName: "fastForwardMediaShortcut"
            sequence: "Media Fast Forward"
            operation: root.actions.seekForward
            scope: InputContext.Seek
            autoRepeat: forward.autoRepeat
            condition: forward.condition
        },
        Binding {
            objectName: "previousChapterShortcut"
            sequence: "PgUp"
            operation: root.actions.previousChapter
            scope: InputContext.Seek
            condition: qsTranslate("Settings", "Recordings with chapters")
        },
        Binding {
            objectName: "nextChapterShortcut"
            sequence: "PgDown"
            operation: root.actions.nextChapter
            scope: InputContext.Seek
            condition: qsTranslate("Settings", "Recordings with chapters")
        },
        Binding {
            objectName: "previousChapterMediaShortcut"
            sequence: "Media Previous"
            operation: root.actions.previousChapter
            scope: InputContext.Seek
            condition: qsTranslate("Settings", "Recordings with chapters")
        },
        Binding {
            id: nextChapterMedia
            objectName: "nextChapterMediaShortcut"
            sequence: "Media Next"
            operation: root.actions.nextChapter
            scope: InputContext.Seek
            condition: qsTranslate("Settings", "Recordings with chapters")
        },
        Binding {
            objectName: "forwardShortcut"
            sequence: "Forward"
            operation: root.actions.nextChapter
            scope: InputContext.Seek
            condition: nextChapterMedia.condition
        },
        Binding {
            objectName: "stopShortcut"
            sequence: "Media Stop"
            operation: root.actions.stopPlayback
            scope: InputContext.Playback
            condition: qsTranslate("Settings", "While something plays")
        },
        Binding { id: dismiss; objectName: "dismissShortcut"; sequence: "Escape"; operation: root.actions.dismissTopmost; scope: InputContext.Dismiss },
        Binding { id: back; objectName: "backShortcut"; sequence: "Back"; operation: root.actions.goBack; scope: InputContext.Dismiss }
    ]
}
