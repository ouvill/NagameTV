"""Author ARIB SI and pace the demo TS; no broadcast payload or hardware."""
from collections import defaultdict
from datetime import datetime, timedelta
import time
from schedule import JST, MILLISECONDS_PER_MINUTE, NETWORK_BASE

PACKET_BYTES = 188
PCR_FREQUENCY = 90000
PCR_WRAP = 1 << 33
SDT_PID, EIT_PID, TOT_PID = 0x11, 0x12, 0x14
JST_MJD_EPOCH = datetime(1858, 11, 17)

def pcr(packet):
    if packet[3] & 32 and packet[4] >= 7 and packet[5] & 16:
        p=packet[6:12]
        return (p[0]<<25)|(p[1]<<17)|(p[2]<<9)|(p[3]<<1)|(p[4]>>7)
    return None

def crc(data):
    value=0xffffffff
    for byte in data:
        value ^= byte << 24
        for _ in range(8):
            value=((value<<1) ^ (0x04c11db7 if value & 0x80000000 else 0)) & 0xffffffff
    return value.to_bytes(4,'big')

def section(table, body, syntax=True):
    data=bytes([table])+((0xf000 if syntax else 0x7000)|(len(body)+4)).to_bytes(2,'big')+body
    return data+crc(data)

def bcd(n):
    return (n//10)*16+n%10

def date(value):
    return (value-JST_MJD_EPOCH).days.to_bytes(2,'big')+bytes(map(bcd,[value.hour,value.minute,value.second]))

def string(raw):
    return bytes([len(raw)])+raw

def descriptor(tag, data):
    return bytes([tag,len(data)])+data

def arib(value):
    # ISO-2022-JP assumes ASCII initially; ARIB starts in Kanji. Designate
    # alphanumeric explicitly so leading markers such as [字] decode correctly.
    return b'\x1b(J' + value.encode('iso2022_jp').replace(b'\x1b(B',b'\x1b(J')

def author(source, target, now, events):
    counts=defaultdict(int)
    def packets(pid, data):
        payload=b'\0'+data
        first=True
        result=bytearray()
        while payload:
            part,payload=payload[:184],payload[184:]
            result+=bytes([0x47,(pid>>8)|(0x40 if first else 0),pid&255,0x10|counts[pid]])+part.ljust(184,b'\xff')
            counts[pid]=(counts[pid]+1)%16
            first=False
        return result
    # ARIB SI wall time is JST, encoded without a timezone offset.
    clock=now.replace(tzinfo=None)
    network=NETWORK_BASE.to_bytes(2,'big')
    desc=descriptor(0x48,b'\x01\0'+string(arib('ながめシネマ')))
    sdt=section(0x42,b'\0\1\xc1\0\0'+network+b'\xff'+b'\0\1\xff'+(0x8000+len(desc)).to_bytes(2,'big')+desc)
    def eit(number):
        event = events[number]
        title = event['name']
        start = datetime.fromtimestamp(event['startAt'] / 1000, JST).replace(tzinfo=None)
        duration_hours, duration_minutes = divmod(event['duration'] // MILLISECONDS_PER_MINUTE, 60)
        duration = bytes((bcd(duration_hours), bcd(duration_minutes), 0))
        info=descriptor(0x4d,b'jpn'+string(arib(title))+string(arib('オープンムービーを楽しむ時間。')))
        info+=descriptor(0x54,bytes((event['genres'][0]['lv1'] << 4, 0)))
        body=b'\0\1\xc1'+bytes([number,1])+b'\0\1'+network+b'\1\x4e'
        body+=event['eventId'].to_bytes(2,'big')+date(start)+duration+(0x8000+len(info)).to_bytes(2,'big')+info
        return section(0x4e,body)
    first=None
    last_second=-1
    with source.open('rb') as src,target.open('wb') as dst:
        while packet:=src.read(PACKET_BYTES):
            if len(packet)!=PACKET_BYTES or packet[0]!=0x47: raise ValueError('Invalid TS packet')
            pid=((packet[1]&31)<<8)|packet[2]
            if pid in (SDT_PID,EIT_PID,TOT_PID): continue
            dst.write(packet)
            timestamp=pcr(packet)
            if timestamp is None: continue
            if first is None: first=timestamp
            second=int(((timestamp-first)%PCR_WRAP)/PCR_FREQUENCY)
            if second==last_second: continue
            last_second=second
            dst.write(packets(SDT_PID,sdt))
            dst.write(packets(EIT_PID,eit(0)))
            dst.write(packets(EIT_PID,eit(1)))
            dst.write(packets(TOT_PID,section(0x73,date(clock+timedelta(seconds=second))+b'\xf0\0',False)))

def stream(source, output, stopping):
    first=None
    started=time.monotonic()
    buffered=bytearray()
    with source.open('rb') as src:
        while packet:=src.read(PACKET_BYTES):
            if stopping.is_set(): break
            buffered+=packet
            timestamp=pcr(packet)
            if timestamp is None: continue
            if first is None: first=timestamp
            elapsed=((timestamp-first)%PCR_WRAP)/PCR_FREQUENCY
            if stopping.wait(max(0,started+elapsed-time.monotonic())): break
            output.write(buffered)
            buffered.clear()
        if buffered and not stopping.is_set(): output.write(buffered)
