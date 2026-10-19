import { test } from "node:test";
import assert from "node:assert/strict";
import { create } from "@bufbuild/protobuf";
import {
  ActivePlaybackSchema,
  BroadcastBand,
  ChannelSchema,
  FilePlaybackSchema,
  PlayerStateSchema,
  ProgramSchema,
  SubtitleDisplay,
  type PlayerState,
} from "../src/gen/viewer/v1/player_pb";
import {
  activeChannelId,
  bands,
  clock,
  controls,
  currentPosition,
  primaryAction,
  programLine,
  progress,
  remoteKeyChannel,
  seekBar,
  steppedRate,
  view,
  wallClock,
} from "../src/model";

type Case = PlayerState["playback"]["case"];
const active = (channelId: bigint) => create(ActivePlaybackSchema, { channelId });
const file = (name: string) => create(FilePlaybackSchema, { name });

function live(c: Exclude<Case, undefined | `file${string}` | "stopped">, extra: Partial<PlayerState> = {}) {
  return create(PlayerStateSchema, {
    playback: { case: c, value: active(7n) },
    playbackRateTenths: 10,
    ...extra,
  } as never);
}

const timeshifted = (c: "playing" | "paused", extra: Partial<PlayerState> = {}) =>
  live(c, {
    canPause: true,
    canSeek: true,
    canReturnToLive: true,
    positionMs: 50_000n,
    seekStartMs: 10_000n,
    seekEndMs: 70_000n,
    ...extra,
  });

test("live playback is labelled by its distance from the edge", () => {
  assert.deepEqual(view(live("playing", { atLiveEdge: true })), { status: "live", badge: "live", channelId: 7n });
  assert.equal(view(live("playing")).status, "timeshift");
  assert.equal(view(live("stopFailed")).badge, "warn");
  assert.deepEqual(view(create(PlayerStateSchema, {})), { status: "stopped", badge: "none" });
});

test("recordings report their file name and never a channel", () => {
  const s = create(PlayerStateSchema, { playback: { case: "filePaused", value: file("a.ts") } });
  assert.deepEqual(view(s), { status: "paused", badge: "muted", file: "a.ts" });
  assert.equal(activeChannelId(s), undefined);
});

test("the highlighted channel prefers the stream target over the catalog selection", () => {
  assert.equal(activeChannelId(live("connecting", { selectedChannelId: 3n })), 7n);
  assert.equal(activeChannelId(create(PlayerStateSchema, { selectedChannelId: 3n })), 3n);
});

test("primary action depends on whether playback can pause", () => {
  assert.equal(primaryAction(create(PlayerStateSchema, {})), "play");
  assert.equal(primaryAction(live("playing")), "stop");
  assert.equal(primaryAction(timeshifted("playing")), "pause");
  assert.equal(primaryAction(timeshifted("paused")), "resume");
  assert.equal(primaryAction(live("ended", { canPause: true })), "play");
});

test("controls hide what the current playback cannot do", () => {
  const plain = controls(live("playing"));
  assert.equal(plain.stopHidden, true, "the primary button already stops");
  assert.equal(plain.skipHidden, true);
  assert.equal(plain.liveHidden, true);
  const shifted = controls(timeshifted("paused"));
  assert.equal(shifted.stopHidden, false);
  assert.equal(shifted.skipHidden, false);
  assert.equal(shifted.liveHidden, false);
  assert.equal(controls(create(PlayerStateSchema, {})).primaryDisabled, true, "nothing selected to play");
  assert.equal(controls(create(PlayerStateSchema, { selectedChannelId: 1n })).primaryDisabled, false);
});

test("subtitle and rate controls follow their limits", () => {
  assert.equal(controls(live("playing", { subtitles: SubtitleDisplay.DISABLED })).subtitlesDisabled, true);
  assert.equal(controls(live("playing", { subtitles: SubtitleDisplay.HIDDEN })).subtitlesOn, false);
  assert.equal(controls(live("playing", { subtitles: SubtitleDisplay.VISIBLE })).subtitlesOn, true);
  assert.equal(controls(live("playing", { playbackRateTenths: 5 })).rateDownDisabled, true);
  assert.equal(controls(live("playing", { playbackRateTenths: 20 })).rateUpDisabled, true);
  assert.equal(steppedRate(10, 1), 11);
  assert.equal(steppedRate(20, 1), undefined);
  assert.equal(steppedRate(5, -1), undefined);
});

test("position advances only while running, scaled by speed and capped at the edge", () => {
  assert.equal(currentPosition(timeshifted("paused"), 5_000), 50_000);
  assert.equal(currentPosition(timeshifted("playing"), 5_000), 55_000);
  assert.equal(currentPosition(timeshifted("playing", { playbackRateTenths: 15 }), 4_000), 56_000);
  assert.equal(currentPosition(timeshifted("playing"), 60_000), 70_000);
  assert.equal(currentPosition(live("playing"), 1_000), undefined);
});

test("live seek bars count back from the edge; recordings count up", () => {
  const behind = seekBar(timeshifted("paused"), 0);
  assert.deepEqual(behind, { min: 10_000, max: 70_000, value: 50_000, totalMs: 60_000, start: { kind: "behind", ms: 20_000 } });
  assert.deepEqual(seekBar(timeshifted("playing", { atLiveEdge: true }), 0)?.start, { kind: "live" });
  const recording = create(PlayerStateSchema, {
    playback: { case: "filePlaying", value: file("a.ts") },
    canSeek: true, positionMs: 30_000n, seekStartMs: 0n, seekEndMs: 90_000n, durationMs: 100_000n,
    playbackRateTenths: 10,
  });
  assert.deepEqual(seekBar(recording, 0), { min: 0, max: 90_000, value: 30_000, totalMs: 100_000, start: { kind: "elapsed", ms: 30_000 } });
  assert.equal(seekBar(live("playing", { canSeek: true }), 0), undefined, "range missing");
});

test("missing programs are reported only when they are not omitted by design", () => {
  const program = create(ProgramSchema, { startAtMs: 1_000n, durationMs: 2_000n });
  assert.deepEqual(programLine(live("playing", { currentProgram: program }), true), { kind: "times", startMs: 1_000, endMs: 3_000 });
  assert.deepEqual(programLine(live("playing"), true), { kind: "missing" });
  assert.deepEqual(programLine(timeshifted("playing"), true), { kind: "none" }, "timeshift omits programs");
  assert.deepEqual(programLine(live("playing"), false), { kind: "none" });
});

test("catalog helpers match remote keys per band", () => {
  const channels = [
    create(ChannelSchema, { id: 1n, band: BroadcastBand.BS, remoteKey: 1 }),
    create(ChannelSchema, { id: 2n, band: BroadcastBand.TERRESTRIAL, remoteKey: 1 }),
  ];
  assert.equal(remoteKeyChannel(channels, BroadcastBand.TERRESTRIAL, 1)?.id, 2n);
  assert.equal(remoteKeyChannel(channels, BroadcastBand.CS, 1), undefined);
  assert.deepEqual(bands(channels), [BroadcastBand.TERRESTRIAL, BroadcastBand.BS]);
});

test("time formatting", () => {
  assert.equal(clock(59_999), "0:59");
  assert.equal(clock(3_725_000), "1:02:05");
  assert.equal(clock(-5), "0:00");
  assert.equal(wallClock(Date.UTC(2026, 0, 1, 9, 5)), "9:05");
  assert.equal(progress(0, 100, 50), 0.5);
  assert.equal(progress(0, 100, 500), 1);
  assert.equal(progress(10, 10, 50), 0);
});
