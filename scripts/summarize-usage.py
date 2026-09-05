#!/usr/bin/env python3
"""Summarize passive diagnostics without attaching to or controlling the viewer."""
import argparse
from collections import Counter
from datetime import datetime
import json
import math
from pathlib import Path


def summarize(path, after=0):
    if path.is_dir():
        candidates = [p for p in path.glob('usage-*.jsonl') if '.previous.' not in p.name]
        if not candidates:
            raise ValueError('No usage logs found')
        path = max(candidates, key=lambda p: p.stat().st_mtime_ns)
    previous = path.with_suffix('.previous.jsonl')
    files = [previous, path] if previous.is_file() else [path]
    counters = Counter()
    metrics = {
        'rss_kib': ('process', 1024),
        'pss_kib': ('process', 1024),
        'private_kib': ('process', 1024),
        'swap_kib': ('process', 1024),
        'virtual_kib': ('process', 1024),
        'anonymous_kib': ('process', 1024),
        'lazy_free_kib': ('process', 1024),
        **{key: ('allocator', 1048576) for key in (
            'arena_bytes', 'in_use_bytes', 'free_bytes', 'mmap_bytes',
            'releasable_top_bytes')},
    }
    memory = {key: [] for key in metrics}
    gc_messages = Counter()
    first = last = None
    dropped = invalid = 0
    for file in files:
        with file.open() as stream:
            for line in stream:
                try:
                    row = json.loads(line)
                    if row.get('kind') == 'qt_gc':
                        if row.get('schema') != 1:
                            raise ValueError('Unsupported GC schema')
                        gc_messages[row['category']] += 1
                        dropped = max(dropped, row['dropped_records'])
                        continue
                    record = row['record']
                    if record['schema'] != 1:
                        raise ValueError('Unsupported schema')
                    dropped = max(dropped, row['dropped_records'])
                    if record['elapsed_ms'] < after * 1000:
                        continue
                    when = record['unix_ms']
                    first = when if first is None else min(first, when)
                    last = when if last is None else max(last, when)
                    counters[record['event']] += 1
                    for key, samples in memory.items():
                        section, divisor = metrics[key]
                        value = (row.get(section) or {}).get(key)
                        if isinstance(value, (int, float)):
                            samples.append((when, value / divisor))
                except (ValueError, KeyError, TypeError, AttributeError):
                    invalid += 1
    if first is None:
        raise ValueError('No supported records found')
    print(f'Log: {path}')
    print(f'Period: {datetime.fromtimestamp(first / 1000).isoformat(timespec="seconds")} — {datetime.fromtimestamp(last / 1000).isoformat(timespec="seconds")}')
    for key, samples in memory.items():
        if samples:
            values = [value for _, value in samples]
            minutes = (samples[-1][0] - samples[0][0]) / 60000
            rate = f'{(values[-1] - values[0]) / minutes:+.3f}' if minutes > 0 else 'unavailable'
            print(f'{key}: first={values[0]:.1f}, last={values[-1]:.1f}, min={min(values):.1f}, max={max(values):.1f} MiB; endpoint_delta/min={rate}')
        else:
            print(f'{key}: unavailable')
    print('GC messages (whole file, including warmup):',
          ', '.join(f'{category}={count}' for category, count in sorted(gc_messages.items())) or 'none recorded')
    print('Allocator counters exclude the QML JS heap and GPU allocations; in_use is not a leak measurement.')
    print('Events:', ', '.join(f'{event}={count}' for event, count in sorted(counters.items())))
    print(f'Dropped records: {dropped}; unreadable/unsupported lines: {invalid}')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('path', type=Path, help='usage directory or a usage-PID.jsonl file')
    parser.add_argument('--after', type=float, default=0, metavar='SECONDS',
                        help='exclude samples before this elapsed time since viewer startup')
    args = parser.parse_args()
    if not math.isfinite(args.after) or args.after < 0:
        parser.error('--after must be nonnegative')
    try:
        summarize(args.path, args.after)
    except (OSError, ValueError) as error:
        parser.exit(1, f'{error}\n')
