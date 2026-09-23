#!/usr/bin/env python3
"""Exercise the style guard's boundaries without Qt or hardware."""
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('ui_style', Path(__file__).with_name('check-ui-style.py'))
guard = importlib.util.module_from_spec(spec)
spec.loader.exec_module(guard)


class StyleGuardTests(unittest.TestCase):
    def check(self, source, name='Screen.qml'):
        return guard.violations(Path(name), source)

    def test_color_cannot_be_hidden_in_a_palette_or_function(self):
        for source in ['palette.text: "#ffffff"', "return '#abc'", 'color: "white"']:
            self.assertEqual(len(self.check(source)), 1)

    def test_comments_urls_and_transparent_do_not_trigger(self):
        self.assertEqual(self.check('''// color: "#fff"; radius: 3
            /* font.pixelSize: 14 */
            source: "https://example.test/icon.svg"
            color: "transparent"
            text: "background: Rectangle { radius: 9 }"
            font.pixelSize: Theme.fontBody
            radius: height / 2'''), [])

    def test_escaped_quotes_cannot_hide_later_assignments(self):
        self.assertEqual(len(self.check(r'text: "say \\"; color: "#fff"')), 1)

    def test_dimensions_and_motion_have_names(self):
        self.assertEqual(len(self.check('radius: 8; font.pixelSize: 14; duration: 100')), 3)

    def test_callers_select_variants_instead_of_overriding_controls(self):
        self.assertEqual(len(self.check('ActionButton { background: Rectangle {} }')), 1)
        self.assertEqual(len(self.check('Viewer.ActionButton { background: Rectangle {} }')), 1)
        self.assertEqual(self.check('ActionButton { emphasis: ActionButton.Primary }'), [])
        self.assertEqual(self.check('ActionButton { Label { background: Rectangle {} } }'), [])

    def test_shared_implementation_and_documented_compositions_are_allowed(self):
        self.assertEqual(self.check('ActionButton { contentItem: Label {} }', 'ActionButton.qml'), [])
        self.assertEqual(self.check('ToggleSwitch { contentItem: RowLayout {} }', 'SettingsToggle.qml'), [])
        self.assertEqual(len(self.check('ToggleSwitch { contentItem: RowLayout {} }')), 1)

    def test_color_definition_files_do_not_grant_other_files_an_exception(self):
        self.assertEqual(self.check('property color accent: "#fff"', 'Theme.qml'), [])
        self.assertEqual(len(self.check('property color accent: "#fff"', 'NewTheme.qml')), 1)


if __name__ == '__main__':
    unittest.main()
