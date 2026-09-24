#!/usr/bin/python3
"""Mux synthetic subtitle fixtures; no decoder or hardware device is used."""
from pathlib import Path
import gi

gi.require_version('Gst', '1.0')
from gi.repository import Gst

Gst.init(None)
root = Path(__file__).resolve().parents[2] / 'tests' / 'fixtures'
for extension, mux in [('mp4', 'mp4mux'), ('mkv', 'matroskamux')]:
    ass = ' appsrc name=ass format=time ! queue ! mux.subtitle_1' if extension == 'mkv' else ''
    pipeline = Gst.parse_launch(
        f'filesrc location="{root}/media-h264.mp4" ! qtdemux name=demux '
        f'{mux} name=mux ! filesink location="{root}/media-subtitles.{extension}" '
        'demux.video_0 ! queue ! mux.video_0 demux.audio_0 ! queue ! mux.audio_0 '
        f'filesrc location="{root}/media-subtitles.srt" ! subparse ! queue ! mux.subtitle_0' + ass)
    if extension == 'mkv':
        source = pipeline.get_by_name('ass')
        header = (root / 'media-subtitles.ass').read_text().split('Dialogue:')[0].encode()
        caps = Gst.Caps.from_string('application/x-ass')
        caps.set_value('codec_data', Gst.Buffer.new_wrapped(header))
        source.set_property('caps', caps)
    pipeline.set_state(Gst.State.PLAYING)
    if extension == 'mkv':
        for index, (start, body) in enumerate([(1, r'{\b1}Styled ASS\N字幕テスト'), (7, r'{\pos(320,192)\c&H00FF00&}Seek and pause')]):
            packet = Gst.Buffer.new_wrapped(f'{index},0,Default,,0,0,0,,{body}'.encode())
            packet.pts = start * Gst.SECOND
            packet.duration = 3 * Gst.SECOND
            assert source.emit('push-buffer', packet) == Gst.FlowReturn.OK
        source.emit('end-of-stream')
    message = pipeline.get_bus().timed_pop_filtered(30 * Gst.SECOND, Gst.MessageType.ERROR | Gst.MessageType.EOS)
    pipeline.set_state(Gst.State.NULL)
    assert message and message.type == Gst.MessageType.EOS, str(message.parse_error() if message else 'timeout')
    print(root / f'media-subtitles.{extension}')
