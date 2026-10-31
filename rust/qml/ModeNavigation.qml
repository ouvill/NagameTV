pragma ComponentBehavior: Bound
import QtQuick
import MinimalViewer

DirectionalFocus {
    id: root
    // None marks the watching screen: it has no button of its own and is reached from home.
    enum Mode { Home, Recording, Guide, Settings, None }
    required property int mode
    required property Window targetWindow
    property bool guideEnabled: false
    property url iconDirectory: "qrc:/qt/qml/MinimalViewer/assets/icons/"
    readonly property real edgeMargin: 18
    readonly property real headerHeight: height + edgeMargin * 2
    signal modeRequested(int mode)
    anchors { right: parent.right; top: parent.top; margins: root.edgeMargin }
    implicitWidth: layout.implicitWidth
    implicitHeight: layout.implicitHeight
    navigationItems: [homeButton, recordingButton, guideButton, settingsButton,
        ...windowButtons.navigationButtons]
    initialItem: mode === ModeNavigation.Recording ? recordingButton
        : mode === ModeNavigation.Guide ? guideButton : mode === ModeNavigation.Settings ? settingsButton : homeButton

    Row {
        id: layout
        spacing: Theme.spaceMd
        Row {
            spacing: Theme.spaceSm
            IconAction {
                id: homeButton
                objectName: "homeModeButton"
                iconSource: root.iconDirectory + "house.svg"
                tip: qsTranslate("Main", "Home")
                flat: true
                focusLabelBelow: true
                active: root.mode === ModeNavigation.Home
                onClicked: root.modeRequested(ModeNavigation.Home)
            }
            IconAction {
                id: recordingButton
                objectName: "recordingModeButton"
                iconSource: root.iconDirectory + "recording.svg"
                tip: qsTranslate("Recording", "Open recording")
                flat: true
                focusLabelBelow: true
                active: root.mode === ModeNavigation.Recording
                // The owner opens the recording library without changing playback.
                onClicked: root.modeRequested(ModeNavigation.Recording)
            }
            IconAction {
                id: guideButton
                objectName: "guideModeButton"
                iconSource: root.iconDirectory + "calendar-days.svg"
                tip: qsTranslate("Main", "Program guide")
                flat: true
                focusLabelBelow: true
                enabled: root.guideEnabled
                active: root.mode === ModeNavigation.Guide
                onClicked: root.modeRequested(ModeNavigation.Guide)
            }
            IconAction {
                id: settingsButton
                objectName: "settingsModeButton"
                iconSource: root.iconDirectory + "settings-2.svg"
                tip: qsTranslate("Main", "Settings")
                flat: true
                focusLabelBelow: true
                active: root.mode === ModeNavigation.Settings
                onClicked: root.modeRequested(ModeNavigation.Settings)
            }
        }
        WindowButtons {
            id: windowButtons
            targetWindow: root.targetWindow
            iconDirectory: root.iconDirectory
            flat: true
            focusLabelBelow: true
        }
    }
}
