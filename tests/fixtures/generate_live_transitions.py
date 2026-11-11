#!/usr/bin/env python3
"""Author live transition fixtures from synthetic media; no hardware is used.

PAT/PMT fit one packet in these inputs. PCR/PTS edits reuse the recording
fixture generator. Codec generation remuxes the existing H.264 MP4 with CPU
GStreamer parsers only; other scenarios use Python's standard library alone.
"""
from pathlib import Path
import subprocess
import tempfile

from generate_recording_recovery import SIZE, crc, pid_at, second_half, write_pid

FIXTURES = Path(__file__).resolve().parent
VIDEO_PID = 0x41
AUDIO_PID = 0x42
PMT_PID = 0x20
PCR_PID = 0x1FE
HALF_TICKS = 288000


def rewrite(data, version, *, pcr=False, audio=False, foreign=False):
    output = bytearray()
    for start in range(0, len(data), SIZE):
        packet = bytearray(data[start:start + SIZE])
        pid = pid_at(packet, 1)
        payload = 4 + (1 + packet[4] if packet[3] & 0x20 else 0)
        if pid in (0, PMT_PID) and packet[1] & 0x40 and packet[3] & 0x10:
            begin = payload + 1 + packet[payload]
            length = 3 + ((packet[begin + 1] & 15) << 8) + packet[begin + 2]
            section = bytearray(packet[begin:begin + length - 4])
            section[5] = 0xC1 | (version << 1)
            if pid == 0:
                if foreign:
                    section[8:10] = b'\x00\x02'
            else:
                if foreign:
                    section[3:5] = b'\x00\x02'
                if pcr:
                    write_pid(section, 8, PCR_PID)
                if audio:
                    # Add a real second AAC ES, retaining its component identity.
                    section.extend(bytes([0x0F, 0xE0, 0x43, 0xF0, 3, 0x52, 1, 0x11]))
            section[1:3] = (0xB000 | (len(section) + 4 - 3)).to_bytes(2, 'big')
            section.extend(crc(section))
            begin = 5
            assert begin + len(section) <= SIZE
            packet[3] = 0x10 | (packet[3] & 15)
            packet[4] = 0
            packet[begin:] = section + bytes([0xFF]) * (SIZE - begin - len(section))
        if pcr and packet[3] & 0x20 and packet[4] >= 7 and packet[5] & 0x10:
            clock = bytearray([0xFF] * SIZE)
            clock[:6] = bytes([0x47, PCR_PID >> 8, PCR_PID & 255, 0x20, 183, packet[5] & 0x90])
            clock[6:12] = packet[6:12]
            output.extend(clock)
            packet[5] &= ~0x10
        output.extend(packet)
        if audio and pid == AUDIO_PID:
            duplicate = bytearray(packet)
            write_pid(duplicate, 1, 0x43)
            output.extend(duplicate)
    # A dedicated PCR PID carries adaptation-only packets, with a stable counter.
    return output


def join(halves):
    """Keep continuity counters continuous; time remains increasing across clips."""
    counters = {}
    output = bytearray()
    for half in halves:
        for start in range(0, len(half), SIZE):
            packet = bytearray(half[start:start + SIZE])
            # Discard the adaptation-only markers inserted by second_half.
            # These cases isolate table changes from a signalled discontinuity.
            if packet[3] & 0x30 == 0x20 and packet[4] == 183 and packet[5] == 0x80:
                continue
            pid = pid_at(packet, 1)
            counter = counters.get(pid, 0)
            packet[3] = (packet[3] & 0xF0) | counter
            if packet[3] & 0x10:
                counters[pid] = (counter + 1) & 15
            output.extend(packet)
    return output


def main():
    original = (FIXTURES / 'recording.ts').read_bytes()
    for name, options in [('version', {}), ('pcr', {'pcr': True}),
                          ('audio', {'audio': True}), ('service', {'foreign': True})]:
        halves = [original]
        halves.append(rewrite(second_half(original, {}, HALF_TICKS), 1, **options))
        halves.append(rewrite(second_half(original, {}, 2 * HALF_TICKS), 2))
        (FIXTURES / f'live-transition-{name}.ts').write_bytes(join(halves))
    with tempfile.TemporaryDirectory() as directory:
        encoded = Path(directory) / 'h264.ts'
        subprocess.run(['gst-launch-1.0', '-q', 'mpegtsmux', 'name=mux', '!',
                        'filesink', f'location={encoded}', 'filesrc',
                        f'location={FIXTURES / "media-h264.mp4"}', '!', 'qtdemux', 'name=demux',
                        'demux.video_0', '!', 'queue', '!', 'h264parse', '!', 'mux.sink_65',
                        'demux.audio_0', '!', 'queue', '!', 'aacparse', '!', 'mux.sink_66'], check=True)
        # H.264 mux starts its PCR/PTS at the same 3600-second origin as recording.ts.
        h264 = rewrite(second_half(encoded.read_bytes(), {}, HALF_TICKS), 1)
        (FIXTURES / 'live-transition-codec.ts').write_bytes(join([original, h264]))


if __name__ == '__main__':
    main()
