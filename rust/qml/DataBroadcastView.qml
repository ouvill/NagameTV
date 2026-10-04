pragma ComponentBehavior: Bound
import QtQuick
import QtWebEngine
import QtWebChannel
import MinimalViewer

WebEngineView {
    id: root
    required property Player backend
    property bool activateOnLoad: true
    property bool browserReady: false
    property bool resumeActivation: true
    readonly property string endpoint: backend.data_broadcast_endpoint
    onEndpointChanged: if (browserReady && endpoint) restartBrowser(activateOnLoad)
    enum Presentation { Opening, Presenting, Transitioning, Standby, Unavailable }
    property int presentation: activateOnLoad ? DataBroadcastView.Opening : DataBroadcastView.Standby
    readonly property bool presenting: presentation !== DataBroadcastView.Standby
    property int activationRevision: 0
    property var videoRect: null
    property string usedKeyList: ""
    property bool inputAvailable: false
    readonly property list<string> usedKeyGroups: inputAvailable && usedKeyList ? usedKeyList.split(" ") : []
    // BML invisible hides the graphics, not its event loop. web-bml awaits a
    // Chromium animation frame during navigation; hiding the WebEngineView
    // would suspend that frame and strand the next document in standby.
    opacity: presenting ? 1 : 0
    enabled: presenting
    function restartBrowser(activateOnLoad) {
        resumeActivation = activateOnLoad;
        activationRevision++;
        browserReady = false;
        videoRect = null;
        usedKeyList = "";
        inputAvailable = false;
        reload();
    }
    function activate() {
        resumeActivation = true;
        activationRevision++;
        presentation = DataBroadcastView.Opening;
        if (browserReady) {
            runJavaScript("window.nagameActivate(" + activationRevision + ")");
        }
    }
    function applyPresentation(snapshot) {
        if (!snapshot || snapshot.revision !== activationRevision) return;
        videoRect = snapshot.videoRect;
        usedKeyList = snapshot.usedKeyList || "";
        inputAvailable = snapshot.inputAvailable === true;
        switch (snapshot.state) {
        case "opening": presentation = DataBroadcastView.Opening; break;
        case "presenting": presentation = DataBroadcastView.Presenting; break;
        case "transitioning": presentation = DataBroadcastView.Transitioning; break;
        case "standby": presentation = DataBroadcastView.Standby; break;
        case "unavailable": presentation = DataBroadcastView.Unavailable; break;
        default: console.error("Unknown data broadcast presentation:", snapshot.state);
        }
    }
    function dataButton() {
        if (browserReady) {
            runJavaScript("window.nagameDataButton()");
        }
        else activate();
    }
    function remoteBack() {
        if (browserReady) runJavaScript("window.nagameRemoteBack && window.nagameRemoteBack()")
    }
    function remoteKey(domKey) {
        if (browserReady)
            runJavaScript("window.nagameRemoteKey && window.nagameRemoteKey(" + JSON.stringify(domKey) + ")")
    }
    signal failed(string reason)
    backgroundColor: "transparent"
    Component.onCompleted: resumeActivation = activateOnLoad
    settings.localContentCanAccessRemoteUrls: true
    settings.playbackRequiresUserGesture: false
    url: "qrc:/qt/qml/MinimalViewer/assets/web-bml/index.html"
    onJavaScriptConsoleMessage: function(level, message, lineNumber, sourceID) {
        root.backend.data_broadcast_console(level, message, sourceID, lineNumber);
    }
    webChannel: WebChannel {
        registeredObjects: [presentationBridge]
    }
    QtObject {
        id: presentationBridge
        WebChannel.id: "presentation"
        // WebChannel owns delivery with this object's lifetime. A pending
        // runJavaScript callback can outlive the QML engine during teardown.
        function update(snapshot: var) { root.applyPresentation(snapshot); }
        function restart(revision: int, activate: bool) {
            if (revision === root.activationRevision) root.restartBrowser(activate);
        }
    }
    onLoadingChanged: function(request) {
        if (request.status === WebEngineView.LoadSucceededStatus) {
            root.browserReady = true;
            const socketUrl = root.backend.data_broadcast_url();
            if (!socketUrl) {
                root.failed(qsTranslate("Viewer", "Data broadcast is unavailable for this stream"));
                return;
            }
            root.runJavaScript("window.nagameConnect(" + JSON.stringify(socketUrl) + ", " + root.activationRevision + ", " + root.resumeActivation + ")");
        } else if (request.status === WebEngineView.LoadFailedStatus) {
            browserReady = false;
            videoRect = null;
            console.error("Load data broadcast browser:", request.errorString);
            failed(request.errorString);
        }
    }
    onRenderProcessTerminated: function(terminationStatus, exitCode) {
        browserReady = false;
        console.error("Data broadcast browser process terminated:", terminationStatus, exitCode);
        failed(qsTranslate("Viewer", "Data broadcast browser stopped"));
    }
}
