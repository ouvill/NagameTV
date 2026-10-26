import "./style.css";
import { client, refreshChannels, run, watch } from "./api";
import { el, icon, tap } from "./dom";
import { locale, t } from "./i18n";
import {
  activeChannelId,
  bands,
  clock,
  controls,
  isRunning,
  primaryAction,
  programLine,
  progress,
  remoteKeyChannel,
  seekBar,
  steppedRate,
  view,
  wallClock,
  type Primary,
  type Status,
} from "./model";
import { BroadcastBand, RemoteKey, type Channel, type PlayerState } from "./gen/viewer/v1/player_pb";

document.documentElement.lang = locale;

let state: PlayerState | undefined;
let channels: Channel[] = [];
let stateReceivedAt = 0;
let numberBand = BroadcastBand.UNSPECIFIED;
let channelFilter: BroadcastBand | "all" = "all";

const elapsed = () => Date.now() - stateReceivedAt;

// ---- feedback ------------------------------------------------------------

const toast = el("div", { class: "toast", role: "status", "aria-live": "polite" });
let toastTimer = 0;
function report(message: string): void {
  toast.textContent = message || t.failed;
  toast.classList.add("visible");
  clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => toast.classList.remove("visible"), 3500);
}

function command(action: () => Promise<unknown>): void {
  tap();
  run(action, report);
}

function button(
  label: string,
  iconName: string,
  onClick: () => void,
  extra: { class?: string; text?: string } = {},
): HTMLButtonElement {
  return el(
    "button",
    { type: "button", class: `btn ${extra.class ?? ""}`, "aria-label": label, title: label, onClick },
    icon(iconName),
    extra.text ? el("span", { class: "btn-text" }, extra.text) : null,
  );
}

// ---- labels --------------------------------------------------------------

const STATUS_TEXT: Record<Status, string> = {
  stopped: t.stopped,
  connecting: t.streamConnecting,
  live: t.live,
  timeshift: t.timeshift,
  paused: t.paused,
  seeking: t.seeking,
  ended: t.ended,
  stopFailed: t.stopFailed,
  recording: t.recording,
};

const PRIMARY: Record<Primary, { icon: string; label: string }> = {
  play: { icon: "play", label: t.play },
  resume: { icon: "play", label: t.play },
  pause: { icon: "pause", label: t.pause },
  stop: { icon: "stop", label: t.stop },
};

function bandName(band: BroadcastBand): string {
  switch (band) {
    case BroadcastBand.TERRESTRIAL: return t.terrestrial;
    case BroadcastBand.BS: return t.bs;
    case BroadcastBand.CS: return t.cs;
    default: return t.other;
  }
}

function channelById(id: bigint | undefined): Channel | undefined {
  return id === undefined ? undefined : channels.find((channel) => channel.id === id);
}

// ---- now playing ---------------------------------------------------------

const connection = el("div", { class: "connection", role: "status" }, t.connecting);

const nowBadge = el("span", { class: "badge" });
const nowChannel = el("span", { class: "now-channel" });
const nowTitle = el("h1", { class: "now-title" });
const nowMeta = el("p", { class: "now-meta" });
const nowFill = el("div", { class: "progress-fill" });
const nowProgress = el("div", { class: "progress" }, nowFill);
const nowError = el("p", { class: "now-error" });
const nowCard = el(
  "section",
  { class: "card now" },
  el("div", { class: "now-head" }, nowBadge, nowChannel),
  nowTitle,
  nowMeta,
  nowProgress,
  nowError,
);

// ---- transport -----------------------------------------------------------

const seekInput = el("input", {
  type: "range", class: "slider", min: "0", max: "1", step: "1000", "aria-label": t.seek,
});
const seekStart = el("span", { class: "time" });
const seekEnd = el("span", { class: "time" });
const seekRow = el("div", { class: "seek" }, seekStart, seekInput, seekEnd);
let seeking = false;
seekInput.addEventListener("pointerdown", () => (seeking = true));
seekInput.addEventListener("input", () => {
  seeking = true;
  seekStart.textContent = clock(Number(seekInput.value) - Number(seekInput.min));
});
seekInput.addEventListener("change", () => {
  const positionMs = BigInt(Math.round(Number(seekInput.value)));
  seeking = false;
  command(() => client.seekTo({ positionMs }));
});

const stopButton = button(t.stop, "stop", () => command(() => client.stop({})), { class: "ghost" });
const back10 = button(t.back10, "back10", () => command(() => client.seekRelative({ deltaMs: -10_000n })));
const playButton = button(t.play, "play", () => {
  if (!state) return;
  switch (primaryAction(state)) {
    case "play": return command(() => client.play({}));
    case "pause": return command(() => client.setPaused({ paused: true }));
    case "resume": return command(() => client.setPaused({ paused: false }));
    case "stop": return command(() => client.stop({}));
  }
}, { class: "primary large" });
const forward10 = button(t.forward10, "forward10", () => command(() => client.seekRelative({ deltaMs: 10_000n })));
const liveButton = button(t.toLive, "live", () => command(() => client.returnToLive({})), {
  class: "ghost", text: t.toLive,
});
const transport = el("div", { class: "transport" }, stopButton, back10, playButton, forward10, liveButton);

const rateValue = el("span", { class: "rate-value" });
function changeRate(step: number): void {
  const tenths = state && steppedRate(state.playbackRateTenths, step);
  if (tenths !== undefined) command(() => client.setPlaybackRate({ tenths }));
}
const rateDown = button(`${t.speed} −`, "left", () => changeRate(-1), { class: "small" });
const rateUp = button(`${t.speed} +`, "right", () => changeRate(1), { class: "small" });
const rateRow = el("div", { class: "rate" }, el("span", { class: "label" }, t.speed), rateDown, rateValue, rateUp);

const muteButton = button(t.mute, "volume", () => {
  if (state) {
    const muted = !state.muted;
    command(() => client.setMuted({ muted }));
  }
}, { class: "small" });
const volumeInput = el("input", {
  type: "range", class: "slider", min: "0", max: "100", step: "1", "aria-label": t.volume,
});
const volumeValue = el("span", { class: "time" });
let volumeDragging = false;
let volumeTimer = 0;
let volumePending: number | undefined;
function flushVolume(): void {
  volumeTimer = 0;
  if (volumePending === undefined) return;
  const fraction = volumePending / 100;
  volumePending = undefined;
  run(() => client.setVolume({ fraction }), report);
}
volumeInput.addEventListener("pointerdown", () => (volumeDragging = true));
volumeInput.addEventListener("input", () => {
  volumeDragging = true;
  volumePending = Number(volumeInput.value);
  volumeValue.textContent = String(volumePending);
  // Throttle so dragging does not fill the viewer's 32-entry command queue.
  if (!volumeTimer) volumeTimer = window.setTimeout(flushVolume, 120);
});
volumeInput.addEventListener("change", () => {
  volumeDragging = false;
  clearTimeout(volumeTimer);
  flushVolume();
});
const subtitlesButton = button(t.subtitles, "subtitles", () => {
  if (state) {
    const visible = !controls(state).subtitlesOn;
    command(() => client.setSubtitles({ visible }));
  }
}, { class: "small toggle" });
const audioRow = el("div", { class: "audio" }, muteButton, volumeInput, volumeValue, subtitlesButton);

const controlsCard = el("section", { class: "card controls" }, seekRow, transport, rateRow, audioRow);

// ---- remote tab ----------------------------------------------------------

function key(k: RemoteKey): () => void {
  return () => command(() => client.sendKey({ key: k }));
}

const dpad = el(
  "div",
  { class: "dpad" },
  button(t.up, "up", key(RemoteKey.UP), { class: "dpad-up" }),
  button(t.left, "left", key(RemoteKey.LEFT), { class: "dpad-left" }),
  el("button", { type: "button", class: "btn dpad-ok", onClick: key(RemoteKey.OK) }, t.ok),
  button(t.right, "right", key(RemoteKey.RIGHT), { class: "dpad-right" }),
  button(t.down, "down", key(RemoteKey.DOWN), { class: "dpad-down" }),
);

const shortcuts = el(
  "div",
  { class: "shortcuts" },
  button(t.keyBack, "back", key(RemoteKey.BACK), { text: t.keyBack }),
  button(t.guide, "guide", key(RemoteKey.GUIDE), { text: t.guide }),
  button(t.channelList, "list", key(RemoteKey.CHANNELS), { text: t.channelList }),
  button(t.fullscreen, "fullscreen", key(RemoteKey.FULLSCREEN), { text: t.fullscreen }),
);

const channelUp = button(t.channelUp, "up", () => command(() => client.stepChannel({ offset: 1 })), {
  text: "CH",
});
const channelDown = button(t.channelDown, "down", () => command(() => client.stepChannel({ offset: -1 })), {
  text: "CH",
});
const lastChannel = button(t.lastChannel, "last", () => command(() => client.previousChannel({})), {
  text: t.lastChannel,
});
const channelRow = el("div", { class: "channel-row" }, channelDown, lastChannel, channelUp);

const bandButtons = new Map<BroadcastBand, HTMLButtonElement>();
const bandRow = el("div", { class: "segmented", role: "group" });
for (const band of [BroadcastBand.TERRESTRIAL, BroadcastBand.BS, BroadcastBand.CS]) {
  const b = el("button", {
    type: "button",
    class: "segment",
    onClick: () => {
      numberBand = band;
      renderNumberPad();
    },
  }, bandName(band));
  bandButtons.set(band, b);
  bandRow.append(b);
}
const numberButtons: HTMLButtonElement[] = [];
const numberPad = el("div", { class: "numpad" });
for (let n = 1; n <= 12; n++) {
  const b = el("button", {
    type: "button",
    class: "btn num",
    onClick: () => command(() => client.selectRemoteKey({ number: n, band: effectiveBand() })),
  }, String(n));
  numberButtons.push(b);
  numberPad.append(b);
}

/// The chosen band, or the watched station's band until one is chosen.
function effectiveBand(): BroadcastBand {
  if (numberBand !== BroadcastBand.UNSPECIFIED) return numberBand;
  return channelById(state && activeChannelId(state))?.band ?? BroadcastBand.TERRESTRIAL;
}

function renderNumberPad(): void {
  const band = effectiveBand();
  for (const [b, node] of bandButtons) node.setAttribute("aria-pressed", String(b === band));
  numberButtons.forEach((node, index) => {
    const channel = remoteKeyChannel(channels, band, index + 1);
    node.title = channel?.name ?? "";
    // Before the catalog arrives, let the viewer decide instead of disabling every key.
    node.disabled = channels.length > 0 && !channel;
  });
}

const remotePanel = el(
  "section",
  { class: "panel", id: "panel-remote", role: "tabpanel" },
  el("div", { class: "card" }, dpad, shortcuts),
  el("div", { class: "card" }, channelRow, bandRow, numberPad),
);

// ---- channels tab --------------------------------------------------------

const filterRow = el("div", { class: "segmented filters", role: "group" });
const channelList = el("ul", { class: "channel-list" });
const channelsPanel = el(
  "section",
  { class: "panel", id: "panel-channels", role: "tabpanel", hidden: true },
  filterRow,
  channelList,
);

// CSP forbids style attributes, so widths go through the CSSOM instead.
function barFill(ratio: number): HTMLSpanElement {
  const fill = el("span", { class: "progress-fill" });
  fill.style.width = `${(ratio * 100).toFixed(1)}%`;
  return fill;
}

function channelItem(channel: Channel, active: bigint | undefined, now: number): HTMLLIElement {
  const program = channel.currentProgram;
  const start = program ? Number(program.startAtMs) : 0;
  const end = program ? start + Number(program.durationMs) : 0;
  return el(
    "li",
    {},
    el(
      "button",
      {
        type: "button",
        class: "channel",
        "aria-current": channel.id === active ? "true" : undefined,
        onClick: () => command(() => client.selectChannel({ channelId: channel.id })),
      },
      el("span", { class: "channel-key" }, channel.remoteKey ? String(channel.remoteKey) : "–"),
      el(
        "span",
        { class: "channel-body" },
        el("span", { class: "channel-name" }, channel.name),
        el("span", { class: "channel-program" }, program?.name || t.noProgram),
        program
          ? el(
              "span",
              { class: "progress thin" },
              barFill(progress(start, end, now)),
            )
          : null,
      ),
      program ? el("span", { class: "channel-time" }, `${wallClock(start)}–${wallClock(end)}`) : null,
    ),
  );
}

function renderChannels(): void {
  const present = bands(channels);
  if (channelFilter !== "all" && !present.includes(channelFilter)) channelFilter = "all";
  filterRow.replaceChildren(
    ...(["all", ...present] as const).map((band) =>
      el("button", {
        type: "button",
        class: "segment",
        "aria-pressed": String(band === channelFilter),
        onClick: () => {
          channelFilter = band;
          renderChannels();
        },
      }, band === "all" ? t.all : bandName(band)),
    ),
  );
  const visible = channels.filter((c) => channelFilter === "all" || c.band === channelFilter);
  if (visible.length === 0) {
    channelList.replaceChildren(el("li", { class: "empty" }, t.noChannels));
    return;
  }
  const active = state && activeChannelId(state);
  const now = Date.now();
  channelList.replaceChildren(...visible.map((channel) => channelItem(channel, active, now)));
}

// ---- tabs ----------------------------------------------------------------

const tabs = [
  { id: "remote", label: t.tabRemote, panel: remotePanel },
  { id: "channels", label: t.tabChannels, panel: channelsPanel },
];
const tabButtons = tabs.map((tab, index) =>
  el("button", {
    type: "button",
    role: "tab",
    class: "tab",
    "aria-controls": tab.panel.id,
    "aria-selected": String(index === 0),
    onClick: () => selectTab(index),
  }, tab.label),
);
function selectTab(index: number): void {
  tabs.forEach((tab, i) => {
    tab.panel.hidden = i !== index;
    tabButtons[i].setAttribute("aria-selected", String(i === index));
  });
  if (tabs[index].id === "channels") run(refreshChannels, report);
}
const tabBar = el("nav", { class: "tabs", role: "tablist" }, ...tabButtons);

document.body.append(
  el("main", { class: "app" }, connection, nowCard, controlsCard, tabBar, remotePanel, channelsPanel),
  toast,
);

// ---- rendering -----------------------------------------------------------

function setIcon(target: HTMLButtonElement, name: string, label: string): void {
  target.querySelector("svg")?.replaceWith(icon(name));
  target.setAttribute("aria-label", label);
  target.title = label;
}

function renderNow(s: PlayerState): void {
  const v = view(s);
  const channel = channelById(activeChannelId(s));
  nowBadge.textContent = STATUS_TEXT[v.status];
  nowBadge.dataset.kind = v.badge;
  nowChannel.textContent = v.file ? t.recording : channel ? `${bandName(channel.band)} · ${channel.name}` : "";
  nowTitle.textContent = v.file ?? s.currentProgram?.name ?? channel?.name ?? t.stopped;
  const line = programLine(s, channel !== undefined);
  nowMeta.textContent = line.kind === "times" ? `${wallClock(line.startMs)}–${wallClock(line.endMs)}`
    : line.kind === "missing" ? t.noProgram : "";
  nowProgress.hidden = line.kind !== "times";
  if (line.kind === "times") {
    nowFill.style.width = `${(progress(line.startMs, line.endMs, Date.now()) * 100).toFixed(1)}%`;
  }
  const error = s.playbackError || s.settingsError;
  nowError.textContent = error;
  nowError.hidden = !error;
}

function renderSeek(s: PlayerState): void {
  const bar = seekBar(s, elapsed());
  seekRow.hidden = bar === undefined;
  if (!bar || seeking) return;
  seekInput.min = String(bar.min);
  seekInput.max = String(bar.max);
  seekInput.value = String(bar.value);
  seekStart.textContent = bar.start.kind === "live" ? t.live
    : bar.start.kind === "behind" ? `−${clock(bar.start.ms)}` : clock(bar.start.ms);
  seekEnd.textContent = clock(bar.totalMs);
}

function renderControls(s: PlayerState): void {
  const c = controls(s);
  setIcon(playButton, PRIMARY[c.primary].icon, PRIMARY[c.primary].label);
  playButton.disabled = c.primaryDisabled;
  stopButton.hidden = c.stopHidden;
  back10.hidden = forward10.hidden = c.skipHidden;
  liveButton.hidden = c.liveHidden;
  rateRow.hidden = c.rateHidden;
  rateValue.textContent = `${(s.playbackRateTenths / 10).toFixed(1)}×`;
  rateDown.disabled = c.rateDownDisabled;
  rateUp.disabled = c.rateUpDisabled;
  setIcon(muteButton, s.muted ? "muted" : "volume", s.muted ? t.unmute : t.mute);
  muteButton.setAttribute("aria-pressed", String(s.muted));
  if (!volumeDragging) {
    const percent = Math.round(s.volumeFraction * 100);
    volumeInput.value = String(percent);
    volumeValue.textContent = String(percent);
  }
  volumeInput.classList.toggle("dimmed", s.muted);
  subtitlesButton.disabled = c.subtitlesDisabled;
  subtitlesButton.setAttribute("aria-pressed", String(c.subtitlesOn));
  channelUp.disabled = channelDown.disabled = c.stepDisabled;
  lastChannel.disabled = c.lastDisabled;
}

function render(): void {
  if (!state) return;
  renderNow(state);
  renderSeek(state);
  renderControls(state);
  renderNumberPad();
}

// Keeps the position readout moving between sparse state updates.
setInterval(() => {
  if (state && isRunning(state)) {
    renderSeek(state);
    renderNow(state);
  }
}, 1000);

void watch({
  state(next) {
    const before = state && activeChannelId(state);
    state = next;
    stateReceivedAt = Date.now();
    if (activeChannelId(next) !== before) renderChannels();
    render();
  },
  channels(next) {
    channels = next;
    renderChannels();
    render();
  },
  connection(status) {
    connection.textContent = status === "retrying" ? t.reconnecting : t.connecting;
    connection.hidden = status === "connected";
    document.body.classList.toggle("offline", status !== "connected");
  },
});
