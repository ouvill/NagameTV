pragma ComponentBehavior: Bound
import QtQuick
import MinimalViewer

Row {
    id: root
    enum Mode { Live, Recording, Guide, Settings }
    required property int mode
    required property Window targetWindow
    property bool guideEnabled: false
    property url iconDirectory: "qrc:/qt/qml/MinimalViewer/assets/icons/"
    readonly property real edgeMargin: 18
    readonly property real headerHeight: height + edgeMargin * 2
    signal modeRequested(int mode)
    anchors { right: parent.right; top: parent.top; margins: root.edgeMargin }
    spacing: Theme.spaceMd

    Row {
        spacing: Theme.spaceXs
        IconAction {
            objectName: "liveModeButton"
            iconSource: root.iconDirectory + "tv.svg"
            tip: qsTranslate("Main", "Live TV")
            flat: true
            active: root.mode === ModeNavigation.Live
            onClicked: root.modeRequested(ModeNavigation.Live)
        }
        IconAction {
            objectName: "recordingModeButton"
            iconSource: root.iconDirectory + "recording.svg"
            tip: qsTranslate("Recording", "Open recording")
            flat: true
            active: root.mode === ModeNavigation.Recording
            // The owner opens the recording library without changing playback.
            onClicked: root.modeRequested(ModeNavigation.Recording)
        }
        IconAction {
            objectName: "guideModeButton"
            iconSource: root.iconDirectory + "calendar-days.svg"
            tip: qsTranslate("Main", "Program guide")
            flat: true
            enabled: root.guideEnabled
            active: root.mode === ModeNavigation.Guide
            onClicked: root.modeRequested(ModeNavigation.Guide)
        }
        IconAction {
            objectName: "settingsModeButton"
            iconSource: root.iconDirectory + "settings-2.svg"
            tip: qsTranslate("Main", "Settings")
            flat: true
            active: root.mode === ModeNavigation.Settings
            onClicked: root.modeRequested(ModeNavigation.Settings)
        }
    }
    WindowButtons {
        targetWindow: root.targetWindow
        iconDirectory: root.iconDirectory
        flat: true
    }
}
