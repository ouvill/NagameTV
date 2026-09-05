#!/usr/bin/env python3
"""Summarize passive diagnostics without attaching to or controlling the viewer."""
import argparse
from collections import Counter
from datetime import datetime
import json
from pathlib import Path


def summarize(path):
    if path.is_dir():
        candidates = [p for p in path.glob('usage-*.jsonl') if '.previous.' not in p.name]
        if not candidates:
            raise ValueError('No usage logs found')
        path = max(candidates, key=lambda p: p.stat().st_mtime_ns)
    previous = path.with_suffix('.previous.jsonl')
    files = [previous, path] if previous.is_file() else [path]
    counters = Counter()
    memory = {key: [] for key in ('rss_kib', 'pss_kib', 'private_kib')}
    first = last = None
    dropped = invalid = 0
    for file in files:
        with file.open() as stream:
            for line in stream:
                try:
                    row = json.loads(line)
                    record = row['record']
                    if record['schema'] != 1:
                        raise ValueError('Unsupported schema')
                    when = record['unix_ms']
                    first = when if first is None else min(first, when)
                    last = when if last is None else max(last, when)
                    counters[record['event']] += 1
                    dropped = max(dropped, row['dropped_records'])
                    for key, samples in memory.items():
                        value = row['process'].get(key)
                        if isinstance(value, (int, float)):
                            samples.append(value / 1024)
                except (ValueError, KeyError, TypeError):
                    invalid += 1
    if first is None:
        raise ValueError('No supported records found')
    print(f'Log: {path}')
    print(f'Period: {datetime.fromtimestamp(first / 1000).isoformat(timespec="seconds")} — {datetime.fromtimestamp(last / 1000).isoformat(timespec="seconds")}')
    for key, samples in memory.items():
        if samples:
            print(f'{key}: first={samples[0]:.1f}, last={samples[-1]:.1f}, min={min(samples):.1f}, max={max(samples):.1f} MiB')
        else:
            print(f'{key}: unavailable')
    print('Events:', ', '.join(f'{event}={count}' for event, count in sorted(counters.items())))
    print(f'Dropped records: {dropped}; unreadable/unsupported lines: {invalid}')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('path', type=Path, help='usage directory or a usage-PID.jsonl file')
    args = parser.parse_args()
    try:
        summarize(args.path)
    except (OSError, ValueError) as error:
        parser.exit(1, f'{error}\n')
