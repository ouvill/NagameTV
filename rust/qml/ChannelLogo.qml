import QtQuick
import MinimalViewer
import QtQuick.Controls

Item {
    id: root
    required property string logoUrl
    implicitWidth: 64
    implicitHeight: 36
    Image {
        id: image
        anchors.fill: parent
        source: root.logoUrl
        sourceSize: Qt.size(128, 72)
        fillMode: Image.PreserveAspectFit
        asynchronous: true
        // Visible delegates own decoded logos; do not retain every visited logo
        // in the global image cache as servers and filters change.
        cache: false
    }
    Label {
        anchors.centerIn: parent
        visible: image.status !== Image.Ready
        text: qsTranslate("Main", "Channel logo")
        font.pixelSize: Theme.fontMicro
        color: Theme.textSecondary
    }
}
