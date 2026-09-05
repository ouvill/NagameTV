"""Pure file tests; no viewer, display, GPU or audio device is used."""
import contextlib
import importlib.util
import io
import json
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location(
    'summarize_usage', Path(__file__).resolve().parents[1] / 'scripts/summarize-usage.py')
summary = importlib.util.module_from_spec(spec)
spec.loader.exec_module(summary)


def sample(seconds, rss, allocator=None):
    row = {
        'record': {'schema': 1, 'unix_ms': seconds * 1000,
                   'elapsed_ms': seconds * 1000, 'event': 'sample'},
        'process': {'rss_kib': rss}, 'dropped_records': 0,
    }
    if allocator is not None:
        row['allocator'] = allocator
    return row


class SummaryTests(unittest.TestCase):
    def test_rotation_gc_old_records_and_warmup(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'usage-123.jsonl'
            previous = path.with_suffix('.previous.jsonl')
            previous.write_text(json.dumps(sample(0, 1024)) + '\n'
                                + json.dumps(sample(300, 2048)) + '\n')
            rows = [
                {'kind': 'qt_gc', 'schema': 1, 'category': 'qt.qml.gc.statistics',
                 'unix_ms': 310000, 'message': 'before\nafter', 'dropped_records': 3},
                sample(360, 3072, {'in_use_bytes': 1048576, 'free_bytes': 2097152}),
                sample(420, 4096, {'in_use_bytes': 2097152, 'free_bytes': 1048576}),
            ]
            path.write_text(''.join(json.dumps(row) + '\n' for row in rows) + '{broken\n')
            output = io.StringIO()
            with contextlib.redirect_stdout(output):
                summary.summarize(Path(directory), after=300)
            text = output.getvalue()
            self.assertIn('rss_kib: first=2.0, last=4.0', text)
            self.assertIn('in_use_bytes: first=1.0, last=2.0', text)
            self.assertIn('endpoint_delta/min=+1.000', text)
            self.assertIn('free_bytes: first=2.0, last=1.0', text)
            self.assertIn('qt.qml.gc.statistics=1', text)
            self.assertIn('Events: sample=3', text)
            self.assertIn('Dropped records: 3; unreadable/unsupported lines: 1', text)

    def test_old_logs_keep_missing_allocator_unknown(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'usage-123.jsonl'
            path.write_text(json.dumps(sample(0, 1024)) + '\n')
            output = io.StringIO()
            with contextlib.redirect_stdout(output):
                summary.summarize(path)
            self.assertIn('in_use_bytes: unavailable', output.getvalue())
            self.assertIn('endpoint_delta/min=unavailable', output.getvalue())
            with self.assertRaises(ValueError):
                summary.summarize(path, after=300)


if __name__ == '__main__':
    unittest.main()
