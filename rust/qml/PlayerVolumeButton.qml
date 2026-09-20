pragma ComponentBehavior: Bound
import QtQuick

IconAction {
    id: root
    required property ViewerActions actions
    property url iconDirectory: "qrc:/qt/qml/MinimalViewer/assets/icons/"
    objectName: "audioSettingsButton"
    flat: true
    active: actions.audioVisible
    iconSource: iconDirectory + (actions.backend.audio_muted || actions.backend.volume_level === 0 ? "volume-x.svg" : "volume-2.svg")
    tip: action.text
    toolTipEnabled: !actions.audioVisible
    action: actions.openAudio
}
