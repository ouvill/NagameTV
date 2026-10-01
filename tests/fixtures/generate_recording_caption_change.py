#!/usr/bin/env python3
"""Generate an ARIB caption/second-audio addition around 15 seconds.

Uses only the checked-in synthetic recording-seek.ts and Python's standard
library. No broadcast payload, display, GPU, or audio device is used.
"""
import argparse
from binascii import crc_hqx
from collections import defaultdict
from enum import Enum
from pathlib import Path

FIXTURES = Path(__file__).resolve().parents[2] / "tests/fixtures"
PACKET_BYTES = 188
HEADER_BYTES = 4
PAYLOAD_BYTES = PACKET_BYTES - HEADER_BYTES
SYNC = 0x47
START = 0x40
PAYLOAD = 0x10
ADAPTATION = 0x20
PCR_FLAG = 0x10
PID_MASK = 0x1fff
COUNTER_MASK = 0x0f
LENGTH_MASK = 0x0fff
CLOCK_HZ = 90_000
CLOCK_WRAP = 1 << 33
CAPTION_START_MS = 15_000
SECOND_AUDIO_START_MS = 15_200
STATEMENT_INTERVAL_MS = 1_000
CAPTION_LEAD_MS = 200
PMT_PID = 0x20
VIDEO_PID = 0x41
AUDIO_PID = 0x42
SECOND_AUDIO_PID = 0x43
CAPTION_PID = 0x130
SUPERIMPOSE_PID = 0x138
STREAM_IDENTIFIER = 0x52
DATA_COMPONENT = 0xfd
ARIB_CAPTION_COMPONENT = 0x0008
CAPTION_COMPONENT_FLAGS = 0x3d
SUPERIMPOSE_COMPONENT_FLAGS = 0x3c
CAPTION_TAG = 0x30
SUPERIMPOSE_TAG = 0x38
PRIVATE_PES_TYPE = 0x06
PRIVATE_STREAM_1 = 0xbd
CRC32_POLYNOMIAL = 0x04c11db7
CRC32_HIGH_BIT = 0x80000000
CRC32_MASK = 0xffffffff


class Scenario(Enum):
    CAPTION_AUDIO = "caption-audio"
    CAPTION_ONLY = "caption-only"


class Stage(Enum):
    SUPERIMPOSE_ONLY = 0
    CAPTION = 1
    SECOND_AUDIO = 2


def pid_of(packet):
    return int.from_bytes(packet[1:3], "big") & PID_MASK


def payload_of(packet):
    if not packet[3] & PAYLOAD:
        return b""
    offset = HEADER_BYTES
    if packet[3] & ADAPTATION:
        offset += 1 + packet[4]
    return packet[offset:]


def pcr_of(packet):
    if packet[3] & ADAPTATION and packet[4] >= 7 and packet[5] & PCR_FLAG:
        return (int.from_bytes(packet[6:10], "big") << 1) | (packet[10] >> 7)
    return None


def crc32(data):
    value = CRC32_MASK
    for byte in data:
        value ^= byte << 24
        for _ in range(8):
            value = ((value << 1) ^ (CRC32_POLYNOMIAL if value & CRC32_HIGH_BIT else 0)) & CRC32_MASK
    return value.to_bytes(4, "big")


def descriptor(tag, data):
    return bytes([tag, len(data)]) + data


def elementary(stream_type, pid, descriptors):
    return (bytes([stream_type]) + (0xe000 | pid).to_bytes(2, "big")
            + (0xf000 | len(descriptors)).to_bytes(2, "big") + descriptors)


def private_stream(pid, tag):
    # ARIB data_component_id 0x0008, plus the caption/superimpose component tag.
    descriptors = descriptor(STREAM_IDENTIFIER, bytes([tag]))
    flags = CAPTION_COMPONENT_FLAGS if tag == CAPTION_TAG else SUPERIMPOSE_COMPONENT_FLAGS
    descriptors += descriptor(DATA_COMPONENT, ARIB_CAPTION_COMPONENT.to_bytes(2, "big") + bytes([flags]))
    return elementary(PRIVATE_PES_TYPE, pid, descriptors)


def program_map(template, stage):
    body = bytearray(template[:-4])
    body[5] = 0xc1 | (stage.value << 1)  # reserved bits, current_next, version
    if stage is Stage.SECOND_AUDIO:
        body += elementary(0x0f, SECOND_AUDIO_PID, descriptor(STREAM_IDENTIFIER, b"\x11"))
    if stage is not Stage.SUPERIMPOSE_ONLY:
        body += private_stream(CAPTION_PID, CAPTION_TAG)
    body += private_stream(SUPERIMPOSE_PID, SUPERIMPOSE_TAG)
    body[1:3] = (0xb000 | (len(body) - 3 + 4)).to_bytes(2, "big")
    return bytes(body) + crc32(body)


def group(identifier, body, superimpose=False):
    # ARIB data_group_id/version, link_number, last_link_number and CRC16.
    data = bytes([identifier << 2, 0, 0]) + len(body).to_bytes(2, "big") + body
    return bytes([0x81 if superimpose else 0x80, 0xff, 0xf0]) + data + crc_hqx(data, 0).to_bytes(2, "big")


def management(superimpose=False):
    # One Japanese language, 960x540 horizontal writing, no data units.
    return group(0, b"\x00\x01\x00jpn\x80\x00\x00\x00", superimpose)


def statement(second):
    # Clear the previous screen, LS1 alphanumeric, entirely authored test text.
    text = b"\x0c\x0e" + f"CAPTION {second:02d}s".encode("ascii")
    unit = b"\x1f\x20" + len(text).to_bytes(3, "big") + text
    return group(1, b"\x00" + len(unit).to_bytes(3, "big") + unit)


def pes(data, pts):
    pts %= CLOCK_WRAP
    encoded = bytes([0x21 | ((pts >> 29) & 14), (pts >> 22) & 255,
                     ((pts >> 14) & 254) | 1, (pts >> 7) & 255, ((pts << 1) & 254) | 1])
    return (bytes([0, 0, 1, PRIVATE_STREAM_1]) + (len(data) + 8).to_bytes(2, "big")
            + b"\x80\x80\x05" + encoded + data)


def generate(scenario):
    # Validate the fixed input before relying on its single-packet PMT and PIDs.
    original = (FIXTURES / "recording-seek.ts").read_bytes()
    if len(original) % PACKET_BYTES:
        raise ValueError("synthetic input has an incomplete transport packet")
    packets = [original[i:i + PACKET_BYTES] for i in range(0, len(original), PACKET_BYTES)]
    template = None
    for packet in packets:
        if packet[0] != SYNC:
            raise ValueError("synthetic input lost transport synchronization")
        pid = pid_of(packet)
        if pid in (SECOND_AUDIO_PID, CAPTION_PID, SUPERIMPOSE_PID):
            raise ValueError("synthetic input already uses a generated PID")
        if pid != PMT_PID:
            continue
        payload = payload_of(packet)
        if not packet[1] & START or not payload:
            raise ValueError("expected a complete single-packet synthetic PMT")
        section = payload[1 + payload[0]:]
        length = 3 + (int.from_bytes(section[1:3], "big") & LENGTH_MASK)
        section = section[:length]
        if len(section) != length or crc32(section) != bytes(4):
            raise ValueError("invalid synthetic PMT length or CRC")
        if template is not None and section != template:
            raise ValueError("expected an unchanging input PMT")
        template = section
    if template is None or int.from_bytes(template[8:10], "big") & PID_MASK != VIDEO_PID:
        raise ValueError("expected the synthetic recording's video PCR PID")

    counters = defaultdict(int)
    output = bytearray()

    def emit(pid, payload):
        first = True
        while payload:
            part, payload = payload[:PAYLOAD_BYTES], payload[PAYLOAD_BYTES:]
            # Adaptation stuffing keeps padding out of PES payloads; it is also
            # valid for PSI. A zero-length adaptation field has no flags byte.
            control = PAYLOAD
            adaptation = b""
            if len(part) < PAYLOAD_BYTES:
                control |= ADAPTATION
                length = PAYLOAD_BYTES - len(part) - 1
                adaptation = bytes([length])
                if length:
                    adaptation += b"\x00" + b"\xff" * (length - 1)
            header = bytes([SYNC, (pid >> 8) | (START if first else 0), pid & 255,
                            control | counters[pid]])
            output.extend(header + adaptation + part)
            counters[pid] = (counters[pid] + 1) & COUNTER_MASK
            first = False

    stage = Stage.SUPERIMPOSE_ONLY
    first_pcr = None
    elapsed_ms = 0
    next_statement_ms = CAPTION_START_MS
    next_superimpose_ms = 0
    # None until a complete first secondary-audio PES can be copied.
    secondary_counter = None
    for packet in packets:
        pid = pid_of(packet)
        if pid == PMT_PID:
            emit(PMT_PID, b"\x00" + program_map(template, stage))
            continue
        output.extend(packet)
        pcr = pcr_of(packet)
        if pid == VIDEO_PID and pcr is not None:
            if first_pcr is None:
                first_pcr = pcr
            elapsed_ms = ((pcr - first_pcr) % CLOCK_WRAP) * 1000 // CLOCK_HZ
            previous = stage
            if stage is Stage.SUPERIMPOSE_ONLY and elapsed_ms >= CAPTION_START_MS:
                stage = Stage.CAPTION
            elif (stage is Stage.CAPTION and scenario is Scenario.CAPTION_AUDIO
                  and elapsed_ms >= SECOND_AUDIO_START_MS):
                stage = Stage.SECOND_AUDIO
            if stage is not previous:
                emit(PMT_PID, b"\x00" + program_map(template, stage))
            if elapsed_ms >= next_superimpose_ms:
                emit(SUPERIMPOSE_PID, pes(management(superimpose=True), pcr))
                next_superimpose_ms += STATEMENT_INTERVAL_MS
            if stage is not Stage.SUPERIMPOSE_ONLY and elapsed_ms >= next_statement_ms:
                pts = pcr + CAPTION_LEAD_MS * CLOCK_HZ // 1000
                emit(CAPTION_PID, pes(management(), pts))
                emit(CAPTION_PID, pes(statement(elapsed_ms // 1000), pts))
                next_statement_ms += STATEMENT_INTERVAL_MS
        if pid == AUDIO_PID and stage is Stage.SECOND_AUDIO:
            if secondary_counter is None:
                if not packet[1] & START:
                    continue
                secondary_counter = 0
            duplicate = bytearray(packet)
            duplicate[1:3] = ((int.from_bytes(packet[1:3], "big") & ~PID_MASK)
                              | SECOND_AUDIO_PID).to_bytes(2, "big")
            duplicate[3] = (packet[3] & ~COUNTER_MASK) | secondary_counter
            if packet[3] & PAYLOAD:
                secondary_counter = (secondary_counter + 1) & COUNTER_MASK
            output.extend(duplicate)
    expected = Stage.SECOND_AUDIO if scenario is Scenario.CAPTION_AUDIO else Stage.CAPTION
    if stage is not expected or first_pcr is None:
        raise ValueError("synthetic input ended before the requested transition")
    return output


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--scenario", choices=[item.value for item in Scenario],
                        default=Scenario.CAPTION_AUDIO.value)
    parser.add_argument("--output", type=Path, default=FIXTURES / "recording-caption-change.ts")
    args = parser.parse_args()
    output = generate(Scenario(args.scenario))
    args.output.write_bytes(output)
    print(f"{args.output}: {len(output)} bytes ({args.scenario})")


if __name__ == "__main__":
    main()
