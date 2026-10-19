// Pure decisions about what the page shows; main.ts only maps them onto the DOM.
import {
  BroadcastBand,
  SubtitleDisplay,
  type Channel,
  type PlayerState,
} from "./gen/viewer/v1/player_pb";

export type Status =
  | "stopped"
  | "connecting"
  | "live"
  | "timeshift"
  | "paused"
  | "seeking"
  | "ended"
  | "stopFailed"
  | "recording";

export type Badge = "live" | "muted" | "warn" | "none";

export type PlaybackView = {
  status: Status;
  badge: Badge;
  channelId?: bigint;
  file?: string;
};

export function view(s: PlayerState): PlaybackView {
  const p = s.playback;
  switch (p.case) {
    case "connecting":
      return { status: "connecting", badge: "muted", channelId: p.value.channelId };
    case "playing":
      return s.atLiveEdge
        ? { status: "live", badge: "live", channelId: p.value.channelId }
        : { status: "timeshift", badge: "muted", channelId: p.value.channelId };
    case "paused":
      return { status: "paused", badge: "muted", channelId: p.value.channelId };
    case "seeking":
    case "seekingPaused":
      return { status: "seeking", badge: "muted", channelId: p.value.channelId };
    case "ended":
      return { status: "ended", badge: "muted", channelId: p.value.channelId };
    case "stopFailed":
      return { status: "stopFailed", badge: "warn", channelId: p.value.channelId };
    case "fileConnecting":
      return { status: "connecting", badge: "muted", file: p.value.name };
    case "filePlaying":
      return { status: "recording", badge: "muted", file: p.value.name };
    case "filePaused":
      return { status: "paused", badge: "muted", file: p.value.name };
    case "fileSeeking":
    case "fileSeekingPaused":
      return { status: "seeking", badge: "muted", file: p.value.name };
    case "fileEnded":
      return { status: "ended", badge: "muted", file: p.value.name };
    case "fileStopFailed":
      return { status: "stopFailed", badge: "warn", file: p.value.name };
    default:
      return { status: "stopped", badge: "none" };
  }
}

/// The channel the page highlights: the stream's target, else the catalog selection.
export function activeChannelId(s: PlayerState): bigint | undefined {
  return view(s).channelId ?? s.selectedChannelId;
}

export function isPaused(s: PlayerState): boolean {
  const c = s.playback.case;
  return c === "paused" || c === "seekingPaused" || c === "filePaused" || c === "fileSeekingPaused";
}

export function isRunning(s: PlayerState): boolean {
  const c = s.playback.case;
  return c === "playing" || c === "seeking" || c === "filePlaying" || c === "fileSeeking";
}

export function isIdle(s: PlayerState): boolean {
  const c = s.playback.case;
  return c === undefined || c === "stopped" || c === "ended" || c === "fileEnded"
    || c === "stopFailed" || c === "fileStopFailed";
}

function isLive(s: PlayerState): boolean {
  const c = s.playback.case;
  return c === "playing" || c === "paused" || c === "seeking" || c === "seekingPaused";
}

/// Advances the last published position by the time since it was received.
export function currentPosition(s: PlayerState, elapsedMs: number): number | undefined {
  if (s.positionMs === undefined) return undefined;
  const base = Number(s.positionMs);
  if (!isRunning(s)) return base;
  const advanced = base + Math.max(0, elapsedMs) * (s.playbackRateTenths / 10);
  const end = s.seekEndMs ?? s.durationMs;
  return end === undefined ? advanced : Math.min(advanced, Number(end));
}

export type SeekBar = {
  min: number;
  max: number;
  value: number;
  start: { kind: "live" } | { kind: "behind"; ms: number } | { kind: "elapsed"; ms: number };
  totalMs: number;
};

export function seekBar(s: PlayerState, elapsedMs: number): SeekBar | undefined {
  if (!s.canSeek || s.seekStartMs === undefined || s.seekEndMs === undefined) return undefined;
  const min = Number(s.seekStartMs);
  const end = Number(s.seekEndMs);
  const value = currentPosition(s, elapsedMs) ?? min;
  const max = Math.max(end, min + 1);
  if (isLive(s)) {
    // Live timelines have no meaningful origin; show how far behind the edge we are.
    return {
      min, max, value, totalMs: end - min,
      start: s.atLiveEdge ? { kind: "live" } : { kind: "behind", ms: end - value },
    };
  }
  const total = (s.durationMs !== undefined ? Number(s.durationMs) : end) - min;
  return { min, max, value, totalMs: total, start: { kind: "elapsed", ms: value - min } };
}

export type Primary = "play" | "pause" | "resume" | "stop";

export function primaryAction(s: PlayerState): Primary {
  if (isIdle(s)) return "play";
  if (s.canPause) return isPaused(s) ? "resume" : "pause";
  return "stop";
}

export type Controls = {
  primary: Primary;
  primaryDisabled: boolean;
  stopHidden: boolean;
  skipHidden: boolean;
  liveHidden: boolean;
  rateHidden: boolean;
  rateDownDisabled: boolean;
  rateUpDisabled: boolean;
  subtitlesDisabled: boolean;
  subtitlesOn: boolean;
  stepDisabled: boolean;
  lastDisabled: boolean;
};

export const MIN_RATE_TENTHS = 5;
export const MAX_RATE_TENTHS = 20;

export function controls(s: PlayerState): Controls {
  const primary = primaryAction(s);
  return {
    primary,
    primaryDisabled: primary === "play" && s.selectedChannelId === undefined && view(s).file === undefined,
    // Without pause the primary button already stops, so a second stop would be redundant.
    stopHidden: isIdle(s) || !s.canPause,
    skipHidden: !s.canSeek,
    liveHidden: !s.canReturnToLive,
    rateHidden: !s.canSetRate,
    rateDownDisabled: s.playbackRateTenths <= MIN_RATE_TENTHS,
    rateUpDisabled: s.playbackRateTenths >= MAX_RATE_TENTHS,
    subtitlesDisabled: s.subtitles === SubtitleDisplay.DISABLED || s.subtitles === SubtitleDisplay.UNSPECIFIED,
    subtitlesOn: s.subtitles === SubtitleDisplay.VISIBLE,
    stepDisabled: !s.canStepChannel,
    lastDisabled: !s.canPreviousChannel,
  };
}

/// The rate after one step, or undefined when it would not change.
export function steppedRate(tenths: number, step: number): number | undefined {
  const next = Math.min(MAX_RATE_TENTHS, Math.max(MIN_RATE_TENTHS, tenths + step));
  return next === tenths ? undefined : next;
}

export type ProgramLine = { kind: "times"; startMs: number; endMs: number } | { kind: "missing" } | { kind: "none" };

export function programLine(s: PlayerState, hasChannel: boolean): ProgramLine {
  const program = s.currentProgram;
  if (view(s).file !== undefined) return { kind: "none" };
  if (program) {
    const startMs = Number(program.startAtMs);
    return { kind: "times", startMs, endMs: startMs + Number(program.durationMs) };
  }
  // Time-shifted sessions omit the program by design, so do not call it missing.
  return hasChannel && !s.canSeek ? { kind: "missing" } : { kind: "none" };
}

export function progress(startMs: number, endMs: number, nowMs: number): number {
  return endMs > startMs ? Math.min(1, Math.max(0, (nowMs - startMs) / (endMs - startMs))) : 0;
}

export function remoteKeyChannel(channels: Channel[], band: BroadcastBand, number: number): Channel | undefined {
  return channels.find((c) => c.band === band && c.remoteKey === number);
}

/// Bands present in the catalog, in enum order, for the filter chips.
export function bands(channels: Channel[]): BroadcastBand[] {
  return [...new Set(channels.map((c) => c.band))].sort((a, b) => a - b);
}

export function clock(ms: number): string {
  const total = Math.max(0, Math.floor(ms / 1000));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const sec = String(total % 60).padStart(2, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${sec}` : `${m}:${sec}`;
}

export function wallClock(ms: number): string {
  const d = new Date(ms);
  return `${d.getHours()}:${String(d.getMinutes()).padStart(2, "0")}`;
}
