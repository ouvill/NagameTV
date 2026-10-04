import { test } from "node:test";
import assert from "node:assert/strict";
import { SessionProtocol } from "../../assets/web-bml/session.js";
import { Presentation, PresentationState, PresentationPublisher } from "../../assets/web-bml/presentation.js";
import { Activation } from "../../assets/web-bml/activation.js";
import { normalizeUsedKeyList, KeyInput } from "../../assets/web-bml/input.js";

test("video documents return keys until a focus or access-key target appears", () => {
    let mutated;
    let accessKey = null;
    let disconnected = 0;
    let notifications = 0;
    const root = { querySelector: () => accessKey };
    const document = { currentFocus: null };
    const browser = { content: { bmlDocument: document },
        getVideoElement: () => ({ getRootNode: () => root }) };
    const input = new KeyInput(browser, () => notifications++, class {
        constructor(callback) { mutated = callback; }
        observe(target) { assert.equal(target, root); }
        disconnect() { disconnected++; }
    });
    input.loaded("A");
    assert.equal(input.available, false);
    document.currentFocus = {};
    mutated();
    assert.equal(input.available, true);
    const previous = notifications;
    mutated(); // Unrelated timer-driven layout changes do not publish geometry.
    assert.equal(notifications, previous);
    document.currentFocus = null;
    mutated();
    assert.equal(input.available, false);
    accessKey = {};
    mutated();
    assert.equal(input.available, true);
    accessKey = null;
    input.loaded("C"); // C profile can establish its first focus with arrows.
    assert.equal(input.available, true);
    browser.getVideoElement = () => null;
    input.loaded("A"); // No DOM observation: preserve the declared mask.
    assert.equal(input.available, true);
    assert.equal(disconnected, 3);
});

test("key masks distinguish defaults, none, and independently requested groups", () => {
    assert.equal(normalizeUsedKeyList(""), "basic data-button");
    assert.equal(normalizeUsedKeyList("none"), "");
    assert.equal(normalizeUsedKeyList("data-button numeric-tuning data-button"), "data-button numeric-tuning");
});
test("automatic startup displays overlays without injecting a data-button request", () => {
    const state = new PresentationState();
    state.reset(false);
    state.synchronized = true;
    state.visibility(false);
    state.navigation(false);
    assert.equal(state.state, Presentation.Presenting);
    assert.equal(state.needsActivation, false);
    state.visibility(true);
    assert.equal(state.state, Presentation.Standby);
    assert.equal(state.needsActivation, false);
});

test("presentation notifications coalesce changes and leave idle pages unscheduled", () => {
    let snapshot = { revision: 1, state: "opening", documentUrl: null, videoRect: null };
    const tasks = [];
    const sent = [];
    let measurements = 0;
    const publisher = new PresentationPublisher(() => { measurements++; return snapshot; },
        value => sent.push(value), task => tasks.push(task));
    publisher.update();
    publisher.update();
    snapshot = { ...snapshot, state: "presenting", documentUrl: "/40/0001/top.bml",
        videoRect: { x: 10, y: 20, width: 320, height: 180 } };
    assert.equal(tasks.length, 1);
    tasks.shift()();
    assert.deepEqual(sent, [snapshot]);
    assert.equal(measurements, 1);
    assert.equal(tasks.length, 0);
    snapshot = { ...snapshot, videoRect: { ...snapshot.videoRect } };
    publisher.update(); tasks.shift()();
    assert.equal(sent.length, 1);
    snapshot = { ...snapshot, videoRect: { ...snapshot.videoRect, width: 640 } };
    publisher.update(); tasks.shift()();
    assert.equal(sent.length, 2);
    snapshot = { ...snapshot, state: "standby", videoRect: null };
    publisher.update(); tasks.shift()();
    assert.equal(sent.length, 3);
    snapshot = { ...snapshot, revision: 2 };
    publisher.update(); tasks.shift()();
    assert.equal(sent.length, 4);
    snapshot = { ...snapshot, usedKeyList: "numeric-tuning" };
    publisher.update(); tasks.shift()();
    assert.equal(sent.length, 5);
    snapshot = { ...snapshot, inputAvailable: true };
    publisher.update(); tasks.shift()();
    assert.equal(sent.length, 6);
    assert.equal(tasks.length, 0);
});

function sink() {
    const events = [];
    return { events, update: m => events.push(m), ready: () => events.push("ready"),
        restart: () => events.push("restart"), fault: reason => events.push(["fault", reason]) };
}
const begin = epoch => ({ type: "begin", version: 1, epoch });
const update = (epoch, message) => ({ type: "update", epoch, message });
test("snapshot, live updates, and reset have separate transitions", () => {
    const target = sink();
    const protocol = new SessionProtocol(target);
    protocol.receive(begin("1"));
    protocol.receive(update("1", "module list"));
    protocol.receive({ type: "ready", epoch: "1" });
    protocol.receive(update("1", "module"));
    protocol.receive(update("0", "obsolete"));
    protocol.receive(begin("2"));
    protocol.receive(update("2", "must wait for new realm"));
    assert.deepEqual(target.events, ["module list", "ready", "module", "restart"]);
});
test("malformed ordering and versions fail instead of reaching web-bml", () => {
    assert.throws(() => new SessionProtocol(sink()).receive(update("1", {})), /before session/);
    assert.throws(() => new SessionProtocol(sink()).receive({ ...begin("1"), version: 2 }), /Unsupported/);
    const protocol = new SessionProtocol(sink());
    protocol.receive(begin("1"));
    assert.throws(() => protocol.receive({ type: "update", message: {} }), /Missing/);
});
test("receiver fault prevents further dispatch and reload loops", () => {
    const target = sink();
    const protocol = new SessionProtocol(target);
    protocol.receive({ type: "fault", reason: "overflow" });
    protocol.receive(begin("1"));
    assert.deepEqual(target.events, [["fault", "overflow"]]);
});
test("activation waits for synchronization and navigation, Back preserves standby", () => {
    const state = new PresentationState();
    state.navigation(false);
    assert.equal(state.needsActivation, false);
    state.synchronized = true;
    assert.equal(state.takeActivation(), true);
    state.visibility(false);
    assert.equal(state.state, Presentation.Presenting);
    state.navigation(true);
    state.visibility(true);
    assert.equal(state.state, Presentation.Transitioning);
    state.navigation(false);
    assert.equal(state.state, Presentation.Standby);
    assert.equal(state.needsActivation, false);
    state.open();
    assert.equal(state.needsActivation, true);
    state.reset(false);
    state.synchronized = true;
    state.navigation(false);
    assert.equal(state.state, Presentation.Standby);
    assert.equal(state.needsActivation, false);
});
test("one activation survives startup navigation but canceled timers cannot resend it", () => {
    let next = 0;
    const tasks = new Map();
    const timer = { setTimeout(fn) { tasks.set(++next, fn); return next; }, clearTimeout(id) { tasks.delete(id); } };
    const tick = () => { const [id, fn] = tasks.entries().next().value; tasks.delete(id); fn(); };
    let pending = true;
    let subscribed = false;
    let presses = 0;
    const activation = new Activation({ pending: () => pending, subscribed: () => subscribed,
        deliver() { pending = false; presses++; } }, timer);
    activation.update();
    tick(); // No subscriber: startup is still waiting for ModuleLocked/timers.
    subscribed = true;
    tick(); // Start quiet interval.
    pending = false; // Navigation supersedes the document.
    activation.update();
    assert.equal(tasks.size, 0);
    pending = true;
    activation.update();
    tick(); tick();
    activation.update();
    assert.equal(presses, 1);
    assert.equal(tasks.size, 0);
});
