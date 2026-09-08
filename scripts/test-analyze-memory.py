#!/usr/bin/env python3
import importlib.util
import json
from pathlib import Path
import tempfile
import sys
sys.dont_write_bytecode = True
import unittest

spec = importlib.util.spec_from_file_location("analyze_memory", Path(__file__).with_name("analyze-memory.py"))
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


def sample(t, rss, event="sample", allocator=None):
    return {"record": {"unix_ms": 1700000000000+t, "elapsed_ms": t, "event": event,
                       "snapshot": {"playing": True}}, "process": {"rss_kib": rss}, "allocator": allocator}


class AnalysisTest(unittest.TestCase):
    def test_rotation_overlap_truncated_tail_and_gc(self):
        with tempfile.TemporaryDirectory() as directory:
            first, second = [Path(directory)/name for name in ("old.jsonl", "new.jsonl")]
            row = sample(0,100)
            first.write_text(json.dumps(row)+"\n")
            second.write_text(json.dumps(row)+"\n"+json.dumps(sample(10000,200))+ '\n{"kind":"qt_gc"}\n{"record":')
            rows,gc,invalid = module.read_samples([second,first])
            self.assertEqual(len(rows),2)
            self.assertEqual(gc,1)
            self.assertEqual(invalid,1)
            self.assertEqual(module.delta(*rows,"process.rss_kib"),100)
            self.assertIsNone(module.delta(*rows,"allocator.free_bytes"))

    def test_report_escapes_event_and_handles_missing_counters(self):
        rows = [sample(0,100),sample(60000,200,"<script>alert(1)</script>")]
        report = module.report(rows,0,0)
        self.assertNotIn("<script>",report)
        self.assertIn("&lt;script&gt;",report)
        self.assertIn("欠測",report)
        self.assertIn("60.0",report)
        self.assertIn("<svg",report)

    def test_single_sample_and_zero(self):
        self.assertIn("RSS最大：0.0",module.report([sample(0,0)],0,0))
        self.assertIn("RSS：欠測",module.report([sample(0,None)],0,0))


if __name__ == "__main__":
    unittest.main()
