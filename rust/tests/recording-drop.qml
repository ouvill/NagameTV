import QtQuick
import QtQuick.Controls
import MinimalViewer

ApplicationWindow {
    id: root
    visible: true
    width: 900
    height: 600
    property bool painted: false
    property string receivedKey: ""
    property string receivedUrl: ""
    property int transfers: 0
    property int files: 0
    onFrameSwapped: painted = true
    Player { id: receiver }
    QtObject {
        id: backend
        property bool recording_loading: false
        property string file_error: ""
        property string playback_error: ""
        property string server: ""
        property bool loading: false
        property string status: ""
        property string settings_error: ""
        signal recordingOpened(bool success)
        signal connectionFinished(bool success, int channels)
        function open_recording_transfer(key) {
            root.receivedKey = key;
            root.transfers++;
            return receiver.open_recording_transfer(key);
        }
        function open_recording(url) { root.receivedUrl = String(url); root.files++; return true; }
    }
    RecordingInput { id: input; anchors.fill: parent; backend: backend }
    FirstRunSetup {
        id: setup
        backend: backend
        onFileDropped: function(file) { input.openUrl(file); }
        onTransferDropped: function(key) { input.openTransfer(key); }
    }
}
