pragma ComponentBehavior: Bound
import QtQuick

Row {
    id: root
    enum Mode { Live, Recording, Guide, Settings }
    required property int mode
    required property Window targetWindow
    property bool guideEnabled: false
    property url iconDirectory: "qrc:/qt/qml/MinimalViewer/assets/icons/"
    signal modeRequested(int mode)
    spacing: 12

    Row {
        spacing: 4
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
            tip: qsTranslate("Recording", "Open TS file")
            flat: true
            active: root.mode === ModeNavigation.Recording
            // The owner chooses the destination: file picker now, library later.
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
