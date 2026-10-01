#!/usr/bin/env python3
"""Add deterministic ARIB SI to the CPU-generated recording-seek.ts fixture.
Run once after regenerating that fixture; no broadcast data or hardware is used.
"""
from pathlib import Path
from datetime import datetime, timedelta
from collections import defaultdict
import sys

path = Path(sys.argv[1] if len(sys.argv) > 1 else 'tests/fixtures/recording-seek.ts')
original = path.read_bytes()
counts = defaultdict(int)
def crc(data):
    value = 0xffffffff
    for byte in data:
        value ^= byte << 24
        for _ in range(8):
            value = ((value << 1) ^ (0x04c11db7 if value & 0x80000000 else 0)) & 0xffffffff
    return value.to_bytes(4, 'big')
def section(table, body, syntax=True):
    prefix = bytes([table]) + ((0xf000 if syntax else 0x7000) | (len(body)+4)).to_bytes(2,'big')
    data = prefix + body
    return data + crc(data)
def packets(pid, section):
    payload = b'\0' + section
    output = b''
    first = True
    while payload:
        part, payload = payload[:184], payload[184:]
        output += bytes([0x47, (pid >> 8) | (0x40 if first else 0), pid&255, 0x10 | counts[pid]]) + part.ljust(184,b'\xff')
        counts[pid] = (counts[pid]+1)%16
        first = False
    return output
def bcd(n): return (n//10)*16 + n%10
def date(value):
    mjd = (value-datetime(1858,11,17)).days
    return mjd.to_bytes(2,'big') + bytes(map(bcd,[value.hour,value.minute,value.second]))
def string(raw): return bytes([len(raw)])+raw
def descriptor(tag, data): return bytes([tag,len(data)])+data
start = datetime(2026,1,1,9)
# Profile A Kanji bytes for 日本語, followed by LS1 ASCII.
title = b'\x46\x7c\x4b\x5c\x38\x6c\x0e'
sdt = section(0x42, b'\0\1\xc1\0\0\0\1\xff' + b'\0\1\xff' + (0x8000+len(descriptor(0x48,b'\x01\0'+string(title+b' TV')))).to_bytes(2,'big') + descriptor(0x48,b'\x01\0'+string(title+b' TV')))
def eit(event, number):
    info = descriptor(0x4d,b'jpn'+string(title+f' {event}'.encode())+string(b'\x0eSynthetic recording information'))
    extended = descriptor(0x4e,b'\x00jpn\0'+string(b'\x0eExtended description'))
    info += extended + descriptor(0x54,b'\x10\0')
    body = b'\0\1\xc1'+bytes([number,1])+b'\0\1\0\1\1\x4e'
    body += event.to_bytes(2,'big') + date(start+timedelta(seconds=(event-1)*30)) + b'\0\0\x30' + (0x8000+len(info)).to_bytes(2,'big') + info
    return section(0x4e,body)
result = bytearray()
first_pcr = None
last_second = -1
for offset in range(0,len(original),188):
    packet = original[offset:offset+188]
    pid = ((packet[1]&31)<<8)|packet[2]
    # Idempotent: replace the generated SI if run again.
    if pid in (0x11,0x12,0x14): continue
    result += packet
    if packet[3]&32 and packet[4]>=7 and packet[5]&16:
        p = packet[6:12]
        pcr = (p[0]<<25)|(p[1]<<17)|(p[2]<<9)|(p[3]<<1)|(p[4]>>7)
        if first_pcr is None: first_pcr = pcr
        elapsed = ((pcr-first_pcr) % (1<<33)) / 90000
        second = int(elapsed)
        if second != last_second:
            last_second = second
            current = 1 if elapsed < 30 else 2
            result += packets(0x11,sdt)
            result += packets(0x12,eit(current,0))
            result += packets(0x12,eit(current+1,1))
            result += packets(0x14,section(0x73,date(start+timedelta(seconds=second))+b'\xf0\0',False))
path.write_bytes(result)
