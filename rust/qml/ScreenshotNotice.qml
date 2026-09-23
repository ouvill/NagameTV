pragma ComponentBehavior: Bound
import QtQuick
import MinimalViewer
import QtQuick.Controls
import QtQuick.Layouts

Control {
    id: root
    enum Kind { Hidden, Saved, Failed }
    property int kind: ScreenshotNotice.Hidden
    property string errorMessage: ""
    property url savedFile: ""
    property int savedCount: 0
    property url deferredFile: ""
    property int deferredCount: 0
    property int timeoutMs: 6000
    property url iconDirectory: "qrc:/qt/qml/MinimalViewer/assets/icons/"
    readonly property string message: kind === ScreenshotNotice.Saved
        ? (savedCount > 1 ? qsTranslate("Main", "%1 screenshots saved.").arg(savedCount)
            : qsTranslate("Main", "Screenshot saved.")) : errorMessage
    signal openFolderRequested(url file)

    function directoryOf(file) { const value = file.toString(); return value.slice(0, value.lastIndexOf("/")); }
    function showSaved(file = "") {
        if (kind === ScreenshotNotice.Failed) {
            deferredCount = deferredCount > 0 && directoryOf(deferredFile) === directoryOf(file)
                ? deferredCount + 1 : 1;
            deferredFile = file;
            return;
        }
        savedCount = kind === ScreenshotNotice.Saved && directoryOf(savedFile) === directoryOf(file)
            ? savedCount + 1 : 1;
        savedFile = file;
        kind = ScreenshotNotice.Saved;
        refreshTimeout();
    }
    function showFailure(message) {
        errorMessage = message;
        kind = ScreenshotNotice.Failed;
        refreshTimeout();
    }
    function dismiss() {
        kind = ScreenshotNotice.Hidden;
        savedCount = 0;
        if (deferredCount > 0) {
            showSaved(deferredFile);
            savedCount = deferredCount;
            deferredFile = "";
            deferredCount = 0;
        }
    }
    function refreshTimeout() {
        if (visible && !hover.hovered && !(folder.visible && folder.activeFocus)) timeout.restart();
        else timeout.stop();
    }

    visible: kind !== ScreenshotNotice.Hidden
    enabled: visible
    onVisibleChanged: refreshTimeout()
    padding: Theme.spaceLg
    implicitHeight: contentItem.implicitHeight + topPadding + bottomPadding
    Accessible.role: Accessible.AlertMessage
    Accessible.name: message
    HoverHandler {
        id: hover
        onHoveredChanged: root.refreshTimeout()
    }
    Timer {
        id: timeout
        interval: root.timeoutMs
        onTriggered: root.dismiss()
    }
    contentItem: RowLayout {
        spacing: Theme.spaceMd
        Label {
            Layout.fillWidth: true
            text: root.message
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
            color: Theme.textPrimary
            font.pixelSize: Theme.fontBody
        }
        IconAction {
            id: folder
            objectName: "screenshotNoticeFolderButton"
            visible: root.kind === ScreenshotNotice.Saved
            enabled: visible
            iconSource: root.iconDirectory + "folder-open.svg"
            tip: qsTranslate("Main", "Open screenshot folder")
            onActiveFocusChanged: root.refreshTimeout()
            onClicked: root.openFolderRequested(root.savedFile)
        }
    }
    background: PanelSurface {}
}
