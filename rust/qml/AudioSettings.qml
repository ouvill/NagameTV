pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Popup {
    id: popup
    required property real windowWidth
    required property real windowHeight
    property bool playing: false
    property string tracksJson: "[]"
    property string errorText: ""
    property url iconDirectory: "qrc:/qt/qml/MinimalViewer/assets/icons/"
    readonly property var tracks: JSON.parse(tracksJson)
    function trackLabel(track, index) {
        const languages = {
            "ja": "日本語",
            "jpn": "日本語",
            "en": "English",
            "eng": "English",
            "de": "Deutsch",
            "deu": "Deutsch",
            "ger": "Deutsch",
            "fr": "Français",
            "fra": "Français",
            "fre": "Français",
            "ko": "한국어",
            "kor": "한국어",
            "zh": "中文",
            "zho": "中文",
            "chi": "中文"
        };
        const language = languages[track.language] || track.language;
        const number = track.number || index + 1;
        const role = track.role === "main" ? qsTranslate("Main", "Main audio") : track.role === "sub" ? qsTranslate("Main", "Sub audio") : track.role === "both" ? qsTranslate("Main", "Main / sub") : "";
        const name = track.role === "both" ? role : (language ? language + (role ? " · " + role : "") : role);
        const duplicate = tracks.some(other => other.id !== track.id && other.language === track.language && other.role === track.role);
        const audioName = qsTranslate("Main", "Audio %1").arg(number);
        return !name ? audioName : duplicate ? name + " · " + audioName : name;
    }
    signal refreshRequested
    signal selectRequested(string trackId)
    parent: Overlay.overlay
    width: Math.min(380, windowWidth - 40)
    height: Math.min(340, windowHeight - 80)
    x: 24
    y: Math.max(20, windowHeight - height - 100)
    padding: 20
    modal: false
    dim: false
    focus: true
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
    onAboutToShow: {
        errorText = "";
        refreshRequested();
    }
    onClosed: tracksJson = "[]"
    Timer {
        interval: 1000
        repeat: true
        running: popup.opened
        onTriggered: popup.refreshRequested()
    }
    background: Rectangle {
        radius: 18
        color: "#f21a1c1a"
        border.color: "#42ffffff"
    }
    contentItem: ColumnLayout {
        spacing: 12
        RowLayout {
            Layout.fillWidth: true
            Label {
                text: qsTranslate("Main", "Audio selection")
                color: "#f4f5f3"
                font.pixelSize: 17
                font.bold: true
                Layout.fillWidth: true
            }
            IconAction {
                iconSource: popup.iconDirectory + "x.svg"
                tip: qsTranslate("Main", "Close")
                implicitWidth: 28
                implicitHeight: 28
                onClicked: popup.close()
            }
        }
        ScrollView {
            id: scroll
            Layout.fillWidth: true
            Layout.fillHeight: true
            contentWidth: availableWidth
            clip: true
            ColumnLayout {
                width: scroll.availableWidth
                spacing: 6
                Repeater {
                    model: popup.tracks
                    delegate: TextAction {
                        id: option
                        objectName: "audioOption" + index
                        required property int index
                        required property var modelData
                        Layout.fillWidth: true
                        enabled: popup.playing && modelData.enabled !== false && popup.tracks.length > 1
                        text: popup.trackLabel(modelData, index)
                        contentItem: Label {
                            text: option.text
                            textFormat: Text.PlainText
                            color: "#f4f5f3"
                            font.pixelSize: 12
                            wrapMode: Text.Wrap
                        }
                        background: Rectangle {
                            radius: 12
                            color: option.modelData.selected ? "#389caf9f" : "#1c1f1c"
                            border.color: option.modelData.selected || option.visualFocus ? "#9caf9f" : "#28ffffff"
                        }
                        onClicked: popup.selectRequested(modelData.id)
                    }
                }
                Label {
                    visible: popup.tracks.length === 0
                    Layout.fillWidth: true
                    text: qsTranslate("Main", "No audio tracks are available yet.")
                    color: "#b6bab6"
                    wrapMode: Text.Wrap
                    font.pixelSize: 12
                }
            }
        }
        Label {
            Layout.fillWidth: true
            visible: popup.tracks.length === 1
            text: qsTranslate("Viewer", "This broadcast has only one audio track.")
            color: "#b6bab6"
            wrapMode: Text.Wrap
            font.pixelSize: 11
        }
        Label {
            Layout.fillWidth: true
            objectName: "audioError"
            visible: popup.errorText.length > 0
            text: popup.errorText
            textFormat: Text.PlainText
            color: "#f4f5f3"
            wrapMode: Text.Wrap
            font.pixelSize: 12
        }
    }
}
