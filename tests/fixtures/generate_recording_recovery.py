#!/usr/bin/env python3
"""Build two six-second recovery cases from the synthetic recording.ts.

Only this known, single-packet PAT/PMT fixture is accepted. No broadcast data,
display, GPU, audio device, or third-party Python package is used.
"""
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
FIXTURES = ROOT / "tests/fixtures"
SIZE = 188
WRAP = 1 << 33


def crc(data):
    value = 0xffffffff
    for byte in data:
        value ^= byte << 24
        for _ in range(8):
            value = ((value << 1) ^ (0x04c11db7 if value & 0x80000000 else 0)) & 0xffffffff
    return value.to_bytes(4, "big")


def pid_at(data, offset):
    return ((data[offset] & 31) << 8) | data[offset + 1]


def write_pid(data, offset, pid):
    data[offset] = (data[offset] & 0xe0) | (pid >> 8)
    data[offset + 1] = pid & 255


def timestamp(data, offset, delta):
    p = data[offset:offset + 5]
    value = ((p[0] & 14) << 29) | (p[1] << 22) | ((p[2] & 254) << 14) | (p[3] << 7) | (p[4] >> 1)
    value = (value + delta) % WRAP
    data[offset:offset + 5] = bytes([
        (p[0] & 0xf1) | ((value >> 29) & 14), (value >> 22) & 255,
        ((value >> 14) & 254) | 1, (value >> 7) & 255, ((value << 1) & 254) | 1,
    ])


def second_half(original, remap, delta):
    output = bytearray()
    marked = set()
    for start in range(0, len(original), SIZE):
        packet = bytearray(original[start:start + SIZE])
        assert len(packet) == SIZE and packet[0] == 0x47
        pid = pid_at(packet, 1)
        target = remap.get(pid, pid)
        # Explicit adaptation-only boundary: preserve payload and its counter.
        if target not in marked:
            marker = bytearray([0xff] * SIZE)
            marker[:6] = bytes([0x47, target >> 8, target & 255,
                                0x20 | ((packet[3] - 1) & 15), 183, 0x80])
            output.extend(marker)
            marked.add(target)
        offset = 4 + (1 + packet[4] if packet[3] & 0x20 else 0)
        if packet[3] & 0x20 and packet[4] >= 7 and packet[5] & 0x10:
            p = packet[6:12]
            pcr = ((p[0] << 25) | (p[1] << 17) | (p[2] << 9) | (p[3] << 1) | (p[4] >> 7))
            pcr = (pcr + delta) % WRAP
            packet[6:11] = bytes([pcr >> 25, (pcr >> 17) & 255, (pcr >> 9) & 255,
                                   (pcr >> 1) & 255, ((pcr & 1) << 7) | (p[4] & 0x7f)])
        if packet[3] & 0x10 and packet[1] & 0x40:
            if pid in (0, 0x20):
                section = offset + 1 + packet[offset]
                size = 3 + (((packet[section + 1] & 15) << 8) | packet[section + 2])
                assert section + size <= SIZE
                if remap:
                    packet[section + 5] = 0xc3  # Current version 1.
                    if pid == 0:
                        assert packet[section + 8:section + 10] == b"\x00\x01"
                        write_pid(packet, section + 10, remap[0x20])
                    else:
                        write_pid(packet, section + 8, remap[pid_at(packet, section + 8)])
                        cursor = section + 12 + (((packet[section + 10] & 15) << 8) | packet[section + 11])
                        while cursor < section + size - 4:
                            write_pid(packet, cursor + 1, remap[pid_at(packet, cursor + 1)])
                            cursor += 5 + (((packet[cursor + 3] & 15) << 8) | packet[cursor + 4])
                    packet[section + size - 4:section + size] = crc(packet[section:section + size - 4])
            elif packet[offset:offset + 3] == b"\x00\x00\x01":
                if packet[offset + 7] & 0x80:
                    timestamp(packet, offset + 9, delta)
                if packet[offset + 7] & 0x40:
                    timestamp(packet, offset + 14, delta)
        write_pid(packet, 1, target)
        output.extend(packet)
    return output


def main():
    original = (FIXTURES / "recording.ts").read_bytes()
    # Same service 1, new PAT/PMT version, new PMT/video/audio/PCR PIDs.
    # Keep time increasing, with a 200ms margin after the three-second first half.
    changed = second_half(original, {0x20: 0x120, 0x41: 0x141, 0x42: 0x142}, 288000)
    (FIXTURES / "recording-pid-change.ts").write_bytes(original + changed)
    # Same tables and PIDs, but the second half restarts its original PCR/PTS/DTS.
    reset = second_half(original, {}, 0)
    (FIXTURES / "recording-clock-reset.ts").write_bytes(original + reset)


if __name__ == "__main__":
    main()
