#!/usr/bin/env python3
"""CPU-only playbin3 output audit for MPEG-2/AAC recovery fixtures.

Requires Python PyGObject and GStreamer libav. Explicit memory sinks and an
in-process decoder allowlist avoid display, GPU and audio device access.
This counts decoded sink input buffers, not images presented or audible samples.
"""
import argparse
import json
from pathlib import Path
import time

import gi

gi.require_version("Gst", "1.0")
from gi.repository import Gst

DEADLINE_SECONDS = 12
BUS_POLL_MS = 100
SECONDS_DECIMAL_PLACES = 3
CPU_DECODERS = ("avdec_mpeg2video", "avdec_aac")
MAX_STREAM_CHANGES = 128
PLAY_FLAGS = "video+audio+soft-volume+buffering+native-video"


def audit(path, paced):
    player = Gst.ElementFactory.make("playbin3")
    if player is None:
        raise RuntimeError("playbin3 is required")
    changes = []
    def element_added(_bin, _subbin, element):
        factory = element.get_factory()
        if factory is None or factory.get_name() != "tsdemux":
            return
        def changed(_demux, pad, action):
            if len(changes) < MAX_STREAM_CHANGES:
                caps = pad.get_current_caps()
                changes.append({"action": action, "pad": pad.get_name(),
                                "caps": caps.to_string() if caps else None})
        element.connect("pad-added", changed, "added")
        element.connect("pad-removed", changed, "removed")
    player.connect("deep-element-added", element_added)
    sinks = {}
    streams = {}
    for medium in ("video", "audio"):
        sink = Gst.ElementFactory.make("fakesink")
        if sink is None:
            raise RuntimeError("memory sink is required")
        sink.props.sync = paced
        player.set_property(medium + "-sink", sink)
        sinks[medium] = sink
        state = {"id": None, "counts": {}}
        streams[medium] = state

        def observe(_pad, info, state=state):
            if info.type & Gst.PadProbeType.EVENT_DOWNSTREAM:
                event = info.get_event()
                if event.type == Gst.EventType.STREAM_START:
                    state["id"] = event.parse_stream_start().rsplit("/", 1)[-1]
            if info.type & Gst.PadProbeType.BUFFER:
                stream_id = state["id"]
                state["counts"][stream_id] = state["counts"].get(stream_id, 0) + 1
            return Gst.PadProbeReturn.OK

        sink.get_static_pad("sink").add_probe(
            Gst.PadProbeType.EVENT_DOWNSTREAM | Gst.PadProbeType.BUFFER, observe
        )
    Gst.util_set_object_arg(player, "flags", PLAY_FLAGS)
    player.props.uri = path.resolve(strict=True).as_uri()
    started = time.monotonic()
    result = {"file": str(path), "paced": paced, "eos": False, "gstreamer": Gst.version_string()}
    try:
        if player.set_state(Gst.State.PLAYING) == Gst.StateChangeReturn.FAILURE:
            raise RuntimeError("playback startup failed")
        bus = player.get_bus()
        while time.monotonic() - started < DEADLINE_SECONDS:
            message = bus.timed_pop_filtered(
                BUS_POLL_MS * Gst.MSECOND, Gst.MessageType.ERROR | Gst.MessageType.EOS
            )
            if message is None:
                continue
            if message.type == Gst.MessageType.ERROR:
                result["error"] = str(message.parse_error())
            elif message.type == Gst.MessageType.EOS:
                result["eos"] = True
            break
        result["elapsed_seconds"] = round(time.monotonic() - started, SECONDS_DECIMAL_PLACES)
        result["final_sink_counters"] = {
            medium: sink.props.stats.get_value("rendered")
            for medium, sink in sinks.items()
        }
        valid, position = player.query_position(Gst.Format.TIME)
        result["position_seconds"] = position / Gst.SECOND if valid else None
    finally:
        # Stop streaming before copying probe counts or leaving on an error.
        player.set_state(Gst.State.NULL)
    result["stream_changes"] = changes
    result["buffers_by_stream"] = {
        medium: state["counts"] for medium, state in streams.items()
    }
    print(json.dumps(result), flush=True)
    return result["eos"]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--paced", action="store_true", help="synchronize memory sinks to the clock")
    parser.add_argument("paths", type=Path, nargs="+")
    args = parser.parse_args()
    Gst.init(None)
    # Rank changes are confined to this diagnostic process, not saved globally.
    for factory in Gst.ElementFactory.list_get_elements(
        Gst.ELEMENT_FACTORY_TYPE_DECODER, Gst.Rank.NONE
    ):
        factory.set_rank(Gst.Rank.NONE)
    for name in CPU_DECODERS:
        factory = Gst.ElementFactory.find(name)
        if factory is None:
            raise RuntimeError(f"required CPU decoder missing: {name}")
        factory.set_rank(Gst.Rank.PRIMARY)
    results = [audit(path, args.paced) for path in args.paths]
    return 0 if all(results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
