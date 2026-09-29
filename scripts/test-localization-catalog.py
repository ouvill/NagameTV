#!/usr/bin/env python3
"""Check application translation sources against the shipped catalog without Qt."""
import json
from pathlib import Path
import re
import unittest
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[1]
STRING = r'"(?:[^"\\]|\\.)*"'


class CatalogTests(unittest.TestCase):
    def setUp(self):
        self.messages = {}
        for context in ET.parse(ROOT / "translations/app_ja.ts").getroot().findall("context"):
            for message in context.findall("message"):
                key = (context.findtext("name"), message.findtext("source"))
                self.assertNotIn(key, self.messages, f"Duplicate translation: {key}")
                self.messages[key] = message.find("translation")

    def assert_translated(self, context, source, location):
        key = (context, source)
        self.assertIn(key, self.messages, f"Missing translation at {location}: {key}")
        translation = self.messages[key]
        self.assertIsNotNone(translation, f"Missing translation text: {key}")
        self.assertNotIn(translation.get("type"), ("unfinished", "vanished", "obsolete"))
        self.assertTrue("".join(translation.itertext()).strip(), f"Empty translation: {key}")

    def test_qml_literal_sources(self):
        pattern = re.compile(rf'qsTranslate\(\s*({STRING})\s*,\s*({STRING})', re.S)
        for path in sorted((ROOT / "rust/qml").glob("*.qml")):
            for match in pattern.finditer(path.read_text()):
                self.assert_translated(*(json.loads(value) for value in match.groups()), path)

    def test_backend_literal_sources(self):
        pattern = re.compile(rf'\b(?:Text::source|Text::message|detail|tr|with_detail)\(\s*({STRING})', re.S)
        for path in sorted((ROOT / "rust/src/player").glob("*.rs")):
            if path.stem.endswith("checks"):
                continue
            for match in pattern.finditer(path.read_text()):
                self.assert_translated("Backend", json.loads(match.group(1)), path)

    def test_intentionally_unchanged_labels(self):
        for context, source in (("Backend", "ニコ実"), ("Backend", "NX"),
                                ("Main", "frames"), ("Viewer", "LIVE")):
            self.assert_translated(context, source, "shared labels")
            self.assertEqual(self.messages[(context, source)].text, source)


if __name__ == "__main__":
    unittest.main()
