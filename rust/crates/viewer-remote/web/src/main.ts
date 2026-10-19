import "./style.css";
import { client, refreshChannels, run, watch } from "./api";
import { el, icon, tap } from "./dom";
import { t } from "./i18n";
import {
  BroadcastBand,
  RemoteKey,
  SubtitleDisplay,
  type Channel,
  type PlayerState,
} from "./gen/viewer/v1/player_pb";

let state: PlayerState | undefined;
let channels: Channel[] = [];
let stateReceivedAt = 0;
let numberBand = BroadcastBand.UNSPECIFIED;
let channelFilter: BroadcastBand | "all" = "all";

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

// ---- playback model ------------------------------------------------------

type View = { status: string; badge: "live" | "muted" | "warn" | "none"; channelId?: bigint; file?: string };

function view(s: PlayerState): View {
  const p = s.playback;
  switch (p.case) {
    case "connecting":
      return { status: t.streamConnecting, badge: "muted", channelId: p.value.channelId };
    case "playing":
      return {
        status: s.atLiveEdge ? t.live : t.timeshift,
        badge: s.atLiveEdge ? "live" : "muted",
        channelId: p.value.channelId,
      };
    case "paused":
      return { status: t.paused, badge: "muted", channelId: p.value.channelId };
    case "seeking":
    case "seekingPaused":
      return { status: t.seeking, badge: "muted", channelId: p.value.channelId };
    case "ended":
      return { status: t.ended, badge: "muted", channelId: p.value.channelId };
    case "stopFailed":
      return { status: t.stopFailed, badge: "warn", channelId: p.value.channelId };
    case "fileConnecting":
      return { status: t.streamConnecting, badge: "muted", file: p.value.name };
    case "filePlaying":
      return { status: t.recording, badge: "muted", file: p.value.name };
    case "filePaused":
      return { status: t.paused, badge: "muted", file: p.value.name };
    case "fileSeeking":
    case "fileSeekingPaused":
      return { status: t.seeking, badge: "muted", file: p.value.name };
    case "fileEnded":
      return { status: t.ended, badge: "muted", file: p.value.name };
    case "fileStopFailed":
      return { status: t.stopFailed, badge: "warn", file: p.value.name };
    default:
      return { status: t.stopped, badge: "none" };
  }
}

function isPaused(s: PlayerState): boolean {
  const c = s.playback.case;
  return c === "paused" || c === "seekingPaused" || c === "filePaused" || c === "fileSeekingPaused";
}

function isRunning(s: PlayerState): boolean {
  const c = s.playback.case;
  return c === "playing" || c === "seeking" || c === "filePlaying" || c === "fileSeeking";
}

function isIdle(s: PlayerState): boolean {
  const c = s.playback.case;
  return c === undefined || c === "stopped" || c === "ended" || c === "fileEnded"
    || c === "stopFailed" || c === "fileStopFailed";
}

/// Advances the last published position locally between state updates.
function currentPosition(s: PlayerState): number | undefined {
  if (s.positionMs === undefined) return undefined;
  const base = Number(s.positionMs);
  if (!isRunning(s)) return base;
  const elapsed = (Date.now() - stateReceivedAt) * (s.playbackRateTenths / 10);
  const end = s.seekEndMs ?? s.durationMs;
  return end === undefined ? base + elapsed : Math.min(base + elapsed, Number(end));
}

function clock(ms: number): string {
  const total = Math.max(0, Math.floor(ms / 1000));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const sec = String(total % 60).padStart(2, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${sec}` : `${m}:${sec}`;
}

function wallClock(ms: number): string {
  const d = new Date(ms);
  return `${d.getHours()}:${String(d.getMinutes()).padStart(2, "0")}`;
}

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

// ---- layout --------------------------------------------------------------

const connection = el("div", { class: "connection", role: "status" }, t.connecting);

const nowBadge = el("span", { class: "badge" });
const nowChannel = el("span", { class: "now-channel" });
const nowTitle = el("h1", { class: "now-title" });
const nowMeta = el("p", { class: "now-meta" });
const nowProgress = el("div", { class: "progress" }, el("div", { class: "progress-fill" }));
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
  const position = BigInt(Math.round(Number(seekInput.value)));
  seeking = false;
  command(() => client.seekTo({ positionMs: position }));
});

const stopButton = button(t.stop, "stop", () => command(() => client.stop({})), { class: "ghost" });
const back10 = button(t.back10, "back10", () => command(() => client.seekRelative({ deltaMs: -10_000n })));
const playButton = button(t.play, "play", () => {
  if (!state) return;
  if (state.canPause && !isIdle(state)) {
    const paused = !isPaused(state);
    command(() => client.setPaused({ paused }));
  } else if (isIdle(state)) {
    command(() => client.play({}));
  } else {
    command(() => client.stop({}));
  }
}, { class: "primary large" });
const forward10 = button(t.forward10, "forward10", () => command(() => client.seekRelative({ deltaMs: 10_000n })));
const liveButton = button(t.toLive, "live", () => command(() => client.returnToLive({})), {
  class: "ghost", text: t.toLive,
});
const transport = el("div", { class: "transport" }, stopButton, back10, playButton, forward10, liveButton);

const rateValue = el("span", { class: "rate-value" });
const rateDown = button(`${t.speed} −`, "left", () => changeRate(-1), { class: "small" });
const rateUp = button(`${t.speed} +`, "right", () => changeRate(1), { class: "small" });
function changeRate(step: number): void {
  if (!state) return;
  const tenths = Math.min(20, Math.max(5, state.playbackRateTenths + step));
  if (tenths !== state.playbackRateTenths) command(() => client.setPlaybackRate({ tenths }));
}
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
    const visible = state.subtitles !== SubtitleDisplay.VISIBLE;
    command(() => client.setSubtitles({ visible }));
  }
}, { class: "small toggle" });
const audioRow = el("div", { class: "audio" }, muteButton, volumeInput, volumeValue, subtitlesButton);

const controlsCard = el("section", { class: "card controls" }, seekRow, transport, rateRow, audioRow);

// Remote tab

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

function selectedBand(): BroadcastBand {
  const s = state;
  const id = s?.selectedChannelId ?? (s ? view(s).channelId : undefined);
  return channelById(id)?.band ?? BroadcastBand.TERRESTRIAL;
}

function effectiveBand(): BroadcastBand {
  return numberBand === BroadcastBand.UNSPECIFIED ? selectedBand() : numberBand;
}

function renderNumberPad(): void {
  const band = effectiveBand();
  for (const [b, node] of bandButtons) node.setAttribute("aria-pressed", String(b === band));
  numberButtons.forEach((node, index) => {
    const channel = channels.find((c) => c.band === band && c.remoteKey === index + 1);
    node.title = channel?.name ?? "";
    node.disabled = channels.length > 0 && !channel;
  });
}

const remotePanel = el(
  "section",
  { class: "panel", id: "panel-remote", role: "tabpanel" },
  el("div", { class: "card" }, dpad, shortcuts),
  el("div", { class: "card" }, channelRow, bandRow, numberPad),
);

// Channels tab

const filterRow = el("div", { class: "segmented filters", role: "group" });
const channelList = el("ul", { class: "channel-list" });
const channelsPanel = el(
  "section",
  { class: "panel", id: "panel-channels", role: "tabpanel", hidden: true },
  filterRow,
  channelList,
);

function renderChannels(): void {
  const bands = [...new Set(channels.map((c) => c.band))].sort((a, b) => a - b);
  if (channelFilter !== "all" && !bands.includes(channelFilter)) channelFilter = "all";
  filterRow.replaceChildren(
    ...(["all", ...bands] as const).map((band) =>
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
  const active = state ? view(state).channelId ?? state.selectedChannelId : undefined;
  const visible = channels.filter((c) => channelFilter === "all" || c.band === channelFilter);
  if (visible.length === 0) {
    channelList.replaceChildren(el("li", { class: "empty" }, t.noChannels));
    return;
  }
  const now = Date.now();
  channelList.replaceChildren(
    ...visible.map((channel) => {
      const program = channel.currentProgram;
      const start = program ? Number(program.startAtMs) : 0;
      const end = program ? start + Number(program.durationMs) : 0;
      const progress = program && end > start ? Math.min(1, Math.max(0, (now - start) / (end - start))) : 0;
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
                  el("span", { class: "progress-fill", style: `width:${(progress * 100).toFixed(1)}%` }),
                )
              : null,
          ),
          program ? el("span", { class: "channel-time" }, `${wallClock(start)}–${wallClock(end)}`) : null,
        ),
      );
    }),
  );
}

// Tabs

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
  const channel = channelById(v.channelId ?? s.selectedChannelId);
  nowBadge.textContent = v.status;
  nowBadge.dataset.kind = v.badge;
  nowChannel.textContent = v.file ? t.recording : channel ? `${bandName(channel.band)} · ${channel.name}` : "";
  const program = s.currentProgram;
  nowTitle.textContent = v.file ?? program?.name ?? channel?.name ?? t.stopped;
  const fill = nowProgress.firstElementChild as HTMLElement;
  if (program && !v.file) {
    const start = Number(program.startAtMs);
    const end = start + Number(program.durationMs);
    nowMeta.textContent = `${wallClock(start)}–${wallClock(end)}`;
    const ratio = end > start ? (Date.now() - start) / (end - start) : 0;
    fill.style.width = `${(Math.min(1, Math.max(0, ratio)) * 100).toFixed(1)}%`;
    nowProgress.hidden = false;
  } else {
    // Timeshifted sessions omit the program by design, so do not call it missing.
    nowMeta.textContent = !program && !v.file && channel && !s.canSeek ? t.noProgram : "";
    nowProgress.hidden = true;
  }
  const error = s.playbackError || s.settingsError;
  nowError.textContent = error;
  nowError.hidden = !error;
}

function renderSeek(s: PlayerState): void {
  const available = s.canSeek && s.seekStartMs !== undefined && s.seekEndMs !== undefined;
  seekRow.hidden = !available;
  if (!available || seeking) return;
  const start = Number(s.seekStartMs);
  const end = Number(s.seekEndMs);
  const position = currentPosition(s) ?? start;
  seekInput.min = String(start);
  seekInput.max = String(Math.max(end, start + 1));
  seekInput.value = String(position);
  const live = s.playback.case === "playing" || s.playback.case === "paused"
    || s.playback.case === "seeking" || s.playback.case === "seekingPaused";
  if (live) {
    // Live timelines have no meaningful origin; show how far behind the edge we are.
    seekStart.textContent = s.atLiveEdge ? t.live : `−${clock(end - position)}`;
    seekEnd.textContent = clock(end - start);
  } else {
    seekStart.textContent = clock(position - start);
    seekEnd.textContent = clock((s.durationMs !== undefined ? Number(s.durationMs) : end) - start);
  }
}

function renderControls(s: PlayerState): void {
  const idle = isIdle(s);
  const paused = isPaused(s);
  if (idle) setIcon(playButton, "play", t.play);
  else if (s.canPause) setIcon(playButton, paused ? "play" : "pause", paused ? t.play : t.pause);
  else setIcon(playButton, "stop", t.stop);
  playButton.disabled = idle && s.selectedChannelId === undefined && view(s).file === undefined;
  // Without pause the main button already stops, so a second stop would be redundant.
  stopButton.hidden = idle || !s.canPause;
  back10.hidden = forward10.hidden = !s.canSeek;
  liveButton.hidden = !s.canReturnToLive;
  rateRow.hidden = !s.canSetRate;
  rateValue.textContent = `${(s.playbackRateTenths / 10).toFixed(1)}×`;
  rateDown.disabled = s.playbackRateTenths <= 5;
  rateUp.disabled = s.playbackRateTenths >= 20;
  setIcon(muteButton, s.muted ? "muted" : "volume", s.muted ? t.unmute : t.mute);
  muteButton.setAttribute("aria-pressed", String(s.muted));
  if (!volumeDragging) {
    const percent = Math.round(s.volumeFraction * 100);
    volumeInput.value = String(percent);
    volumeValue.textContent = String(percent);
  }
  volumeInput.classList.toggle("dimmed", s.muted);
  subtitlesButton.disabled = s.subtitles === SubtitleDisplay.DISABLED || s.subtitles === SubtitleDisplay.UNSPECIFIED;
  subtitlesButton.setAttribute("aria-pressed", String(s.subtitles === SubtitleDisplay.VISIBLE));
  channelUp.disabled = channelDown.disabled = !s.canStepChannel;
  lastChannel.disabled = !s.canPreviousChannel;
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
    const before = state ? view(state).channelId ?? state.selectedChannelId : undefined;
    state = next;
    if ((view(next).channelId ?? next.selectedChannelId) !== before) renderChannels();
    stateReceivedAt = Date.now();
    document.body.classList.remove("offline");
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
