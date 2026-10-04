// Browser-only adapter: session control, public web-bml API, and Qt presentation.
import { BMLBrowser, keyCodeToAribKey } from "web-bml";
import { Presentation, PresentationState, PresentationPublisher } from "./presentation.js";
import { Activation, hasDataButtonHandler } from "./activation.js";
import { SessionProtocol } from "./session.js";
import { usedKeyList, KeyInput } from "./input.js";

const content = document.getElementById("content");
const media = document.getElementById("media");
const status = document.getElementById("status");
const japanese = navigator.language.toLowerCase().startsWith("ja");
const presentation = new PresentationState();
let documentUrl = null;
let resolveBridge;
const bridgeReady = new Promise(resolve => { resolveBridge = resolve; });
let restarting = false;
let retryTask = null;
let programKnown = false;
let entryAvailable = false;
let activationRevision = 0;
let publishPresentation = () => {};
let acceptedKeys = "";
let inputAvailable = false;
const browser = new BMLBrowser({
    containerElement: content,
    mediaElement: media,
    tabIndex: 0,
    storagePrefix: "nagametv_",
    videoPlaneModeEnabled: true,
    indicator: {
        setUrl(name, loading) {
            documentUrl = name;
            presentation.navigation(loading);
            if (!loading) keyInput.refresh();
            updatePresentation();
        },
        setReceivingStatus(receiving) {
            presentation.reception(receiving);
            updatePresentation();
        },
        setNetworkingGetStatus() {},
        setNetworkingPostStatus() {},
        setEventName() {},
    },
});

let resolution = null;
let videoElement = null;
let videoVisible = false;
const keyInput = new KeyInput(browser, () => publishPresentation());
// web-bml owns the BML video object's geometry. Qt reads its viewport rectangle
// and positions the native GStreamer item beneath the transparent video plane.
window.nagameVideoRect = () => {
    if (!videoVisible || videoElement == null || !videoElement.isConnected) return null;
    const rect = videoElement.getBoundingClientRect();
    if (rect.width <= 0 || rect.height <= 0) return null;
    return { x: rect.x, y: rect.y, width: rect.width, height: rect.height };
};
const activation = new Activation({
    pending: () => socket !== null && !restarting && presentation.needsActivation,
    subscribed: () => hasDataButtonHandler(browser),
    deliver() {
        if (!presentation.takeActivation()) return;
        window.nagameRemoteKey("d");
        updatePresentation();
    },
});
function updatePresentation() {
    activation.update();
    if (presentation.state === Presentation.Unavailable) {
        status.textContent = japanese ? "この番組にはデータ放送がありません" : "This program has no data broadcast";
        status.style.display = "block";
    } else if (presentation.state === Presentation.Opening) {
        status.textContent = japanese ? "データ放送を受信中…" : "Receiving data broadcast…";
        status.style.display = "block";
    } else {
        status.style.display = "none";
    }
    publishPresentation();
}
window.nagamePresentation = () => {
    // Preserve ownership while scripts/resources of the next document load.
    if (!presentation.loading) inputAvailable = keyInput.available;
    return {
        revision: activationRevision,
        state: presentation.state,
        documentUrl,
        usedKeyList: acceptedKeys,
        inputAvailable,
        videoRect: [Presentation.Standby, Presentation.Unavailable].includes(presentation.state) ? null : window.nagameVideoRect(),
    };
};
// Only presentation state crosses WebChannel. Broadcast resources still use
// web-bml's WebSocket schema. Page-owned notifications end with the page, so no Qt
// JavaScript result callback can arrive after the QML engine is destroyed.
new QWebChannel(qt.webChannelTransport, (channel) => {
    resolveBridge(channel.objects.presentation);
    const publisher = new PresentationPublisher(
        window.nagamePresentation, snapshot => channel.objects.presentation.update(snapshot));
    publishPresentation = () => publisher.update();
    publishPresentation();
});
function layout() {
    if (resolution == null) return;
    const scale = Math.min(window.innerWidth / resolution.width, window.innerHeight / resolution.height);
    content.style.width = `${resolution.width}px`;
    content.style.height = `${resolution.height}px`;
    content.style.position = "absolute";
    content.style.left = `${(window.innerWidth - resolution.width * scale) / 2}px`;
    content.style.top = `${(window.innerHeight - resolution.height * scale) / 2}px`;
    content.style.transformOrigin = "top left";
    content.style.transform = `scale(${scale})`;
    publishPresentation();
}
browser.addEventListener("videochanged", () => publishPresentation());
browser.addEventListener("usedkeylistchanged", () => {
    acceptedKeys = usedKeyList(browser);
    publishPresentation();
});
browser.addEventListener("load", (event) => {
    keyInput.loaded(event.detail.profile);
    resolution = event.detail.resolution;
    layout();
    videoElement = browser.getVideoElement();
    videoVisible = !browser.content.invisible;
    console.debug("BML document loaded", browser.content.invisible ? "invisible" : "visible");
    presentation.visibility(browser.content.invisible !== false);
    updatePresentation();
});
browser.addEventListener("invisible", (event) => {
    videoVisible = !event.detail;
    console.debug("BML visibility", event.detail ? "invisible" : "visible");
    // web-bml measures the next document's video cutout before making it
    // visible. Keep layout active while hidden so it never measures a zero
    // rectangle. Opacity also hides descendants that force visibility:visible.
    content.style.opacity = event.detail ? "0" : "1";
    content.inert = event.detail;
    // Delay the observation until a synchronous launchDocument following a
    // visibility change has had a chance to mark the new document as loading.
    queueMicrotask(() => {
        presentation.visibility(event.detail);
        updatePresentation();
    });
});
window.addEventListener("resize", layout);

let socket = null;
window.nagameSocketReady = false;
window.nagameActivate = (revision = activationRevision) => {
    activationRevision = revision;
    presentation.open();
    status.textContent = japanese
        ? "データ放送を受信中…"
        : "Receiving data broadcast…";
    status.style.display = "block";
    updatePresentation();
};
window.nagameDataButton = () => {
    if (!presentation.synchronized || presentation.loading || presentation.activationPending) {
        // Retain one user request while startup/navigation is still in flight.
        window.nagameActivate();
    } else {
        window.nagameRemoteKey("d");
    }
};
function restart() {
    if (restarting) return;
    restarting = true;
    activation.cancel();
    if (retryTask !== null) clearTimeout(retryTask);
    // The QML owner reloads the whole page, including the interpreter and audio.
    // Resume only a key request that has not been delivered. Visible overlays
    // do not reveal whether the user opened or closed a data menu.
    const revision = activationRevision;
    const activate = presentation.activationPending;
    bridgeReady.then(bridge => bridge.restart(revision, activate));
}
window.nagameConnect = (url, revision = activationRevision, activate = true) => {
    if (socket) { socket.onclose = null; socket.close(); }
    if (retryTask !== null) clearTimeout(retryTask);
    activation.cancel();
    window.nagameSocketReady = false;
    programKnown = false;
    entryAvailable = false;
    acceptedKeys = "";
    inputAvailable = false;
    activationRevision = revision;
    presentation.reset(activate);
    updatePresentation();
    const connection = new WebSocket(url);
    socket = connection;
    let faulted = false;
    let startupPolicyPending = true;
    const protocol = new SessionProtocol({
        update(message) {
            if (message.type === "pmt") {
                entryAvailable = message.components.some(component => component.streamType === 0x0d
                    && (component.componentId === 0x40 || component.componentId === 0x80));
                const entry = message.components.find(component => component.bxmlInfo?.entryPointInfo);
                // TR-B14 2.1.10.4: for a manual-start service the first d starts
                // the engine. It must not also toggle the startup document off.
                if (startupPolicyPending && entry?.bxmlInfo.entryPointInfo.autoStartFlag === false) {
                    presentation.activationPending = false;
                }
                if (entry) startupPolicyPending = false;
            } else if (message.type === "programInfo") {
                programKnown = message.transportStreamId != null;
            }
            browser.emitMessage(message);
            if (message.type === "pmt" || message.type === "programInfo") {
                presentation.availability(programKnown ? entryAvailable : null);
                updatePresentation();
            }
        },
        ready() { presentation.synchronized = true; updatePresentation(); },
        restart,
        fault(reason) {
            faulted = true;
            activation.cancel();
            if (reason === "closed") console.debug("Data broadcast session closed");
            else console.error("Data broadcast receiver stopped:", reason);
            status.textContent = japanese ? "データ放送の受信が停止しました。開き直してください" : "Data broadcast reception stopped. Close and reopen it.";
            status.style.display = "block";
            connection.close();
        },
    });
    connection.onopen = () => {
        if (socket === connection) window.nagameSocketReady = true;
    };
    connection.onmessage = (event) => {
        if (socket !== connection || restarting) return;
        try { protocol.receive(JSON.parse(event.data)); }
        catch (error) {
            faulted = true;
            activation.cancel();
            console.error("Handle data broadcast message:", error);
            status.textContent = japanese ? "データ放送の受信データを処理できませんでした" : "Could not process data broadcast content";
            status.style.display = "block";
            connection.close();
        }
    };
    connection.onerror = () => console.error("Data broadcast WebSocket failed");
    connection.onclose = () => {
        if (socket !== connection) return;
        activation.cancel();
        socket = null;
        window.nagameSocketReady = false;
        if (faulted || restarting) return;
        status.textContent = japanese ? "データ放送に再接続中…" : "Reconnecting data broadcast…";
        status.style.display = "block";
        retryTask = setTimeout(restart, 1000);
    };
};

window.nagameReady = true;
window.nagameRemoteKey = (domKey) => {
    const key = keyCodeToAribKey(domKey);
    if (key === -1) return;
    browser.content.processKeyDown(key);
    browser.content.processKeyUp(key);
};
window.nagameRemoteBack = () => window.nagameRemoteKey("Backspace");

window.addEventListener("keydown", (event) => {
    // web-bml already handles events originating in its shadow-root document.
    if (event.composedPath().includes(content)) return;
    if (event.altKey || event.ctrlKey || event.metaKey) return;
    const key = keyCodeToAribKey(event.key);
    if (key === -1) return;
    event.preventDefault();
    browser.content.processKeyDown(key);
});
window.addEventListener("keyup", (event) => {
    if (event.composedPath().includes(content)) return;
    if (event.altKey || event.ctrlKey || event.metaKey) return;
    const key = keyCodeToAribKey(event.key);
    if (key === -1) return;
    event.preventDefault();
    browser.content.processKeyUp(key);
});
