pragma ComponentBehavior: Bound
import QtQuick
import MinimalViewer 1.0 as Native

Item {
    id: root
    required property ViewerActions actions
    required property InputContext inputContext
    Native.ShortcutKey { id: keyMatcher }
    // Linux IBus can forward keys directly to the focus object, bypassing
    // Qt's window shortcut map. Normal shortcut presses are consumed before
    // Keys.pressed, so this path must use the same enabled bindings exactly once.
    Connections {
        target: root.inputContext.focusItem ? root.inputContext.focusItem.Keys : null
        function onPressed(event) {
            if (!root.inputContext.targetWindow.active) return;
            const matches = root.entries.filter(binding => binding.enabled
                && keyMatcher.matches(binding.portableText, event.key, event.modifiers));
            if (matches.length === 0) return;
            event.accepted = true;
            if (matches.length > 1) {
                console.warn("Ambiguous shortcut: " + matches[0].portableText);
                return;
            }
            const binding = matches[0];
            if (!event.isAutoRepeat || binding.autoRepeat) binding.operation.trigger();
        }
    }
    component Binding: ShortcutBinding {
        inputContext: root.inputContext
        active: root.enabled
    }
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
        Binding { id: previous; objectName: "previousChannelShortcut"; sequence: "PgUp"; operation: root.actions.previousChannel; scope: InputContext.Navigation },
        Binding { id: next; objectName: "nextChannelShortcut"; sequence: "PgDown"; operation: root.actions.nextChannel; scope: InputContext.Navigation },
        Binding {
            id: playback
            objectName: "playbackShortcut"
            sequence: "Space"
            operation: root.actions.playbackToggle
            scope: InputContext.Playback
            description: qsTranslate("Settings", "Play or pause")
            condition: qsTranslate("Settings", "Recordings and pausable live TV")
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
        Binding { id: dismiss; objectName: "dismissShortcut"; sequence: "Escape"; operation: root.actions.dismissTopmost; scope: InputContext.Dismiss }
    ]
}
