pragma ComponentBehavior: Bound
import QtQuick
import MinimalViewer
import QtQuick.Controls
import QtQuick.Layouts

// Brief confirmation on the TV that a phone or other remote client acted.
Control {
    id: root
    required property var backend
    property int timeoutMs: 2500
    property string message: ""
    property url iconDirectory: "qrc:/qt/qml/MinimalViewer/assets/icons/"

    // Key forwarding and seeks are shown by the UI they drive, so they have no message.
    function messageFor(kind: int): string {
        switch (kind) {
        case Player.RemoteCommandSelectChannel:
        case Player.RemoteCommandStepChannel:
        case Player.RemoteCommandPreviousChannel:
        case Player.RemoteCommandSelectRemoteKey:
            return qsTranslate("Remote", "Channel changed");
        case Player.RemoteCommandPlay: return qsTranslate("Remote", "Play");
        case Player.RemoteCommandStop: return qsTranslate("Remote", "Stopped");
        case Player.RemoteCommandSetVolume:
            return qsTranslate("Remote", "Volume %1%").arg(Math.round(backend.volume_level * 100));
        case Player.RemoteCommandSetMuted:
            return backend.audio_muted ? qsTranslate("Remote", "Muted") : qsTranslate("Remote", "Unmuted");
        case Player.RemoteCommandSetSubtitles:
            return backend.subtitle_display ? qsTranslate("Remote", "Subtitles on") : qsTranslate("Remote", "Subtitles off");
        case Player.RemoteCommandSetPaused:
            return backend.paused ? qsTranslate("Remote", "Paused") : qsTranslate("Remote", "Resumed");
        case Player.RemoteCommandReturnToLive: return qsTranslate("Remote", "Returned to live");
        case Player.RemoteCommandSetPlaybackRate:
            return qsTranslate("Remote", "Speed %1×").arg((backend.playback_rate / 10).toFixed(1));
        }
        return "";
    }
    function show(kind: int) {
        const text = messageFor(kind);
        if (text.length === 0) return;
        message = text;
        timeout.restart();
    }

    visible: timeout.running
    padding: Theme.spaceMd
    leftPadding: Theme.spaceLg
    rightPadding: Theme.spaceLg
    Accessible.role: Accessible.AlertMessage
    Accessible.name: message
    Timer { id: timeout; interval: root.timeoutMs }
    contentItem: RowLayout {
        spacing: Theme.spaceMd
        Image {
            source: root.iconDirectory + "remote-control.svg"
            sourceSize: Qt.size(20, 20)
            Layout.preferredWidth: 20
            Layout.preferredHeight: 20
        }
        Label {
            objectName: "remoteNoticeText"
            text: root.message
            textFormat: Text.PlainText
            color: Theme.textPrimary
            font.pixelSize: Theme.fontBody
        }
    }
    background: PanelSurface {}
}
