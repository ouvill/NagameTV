#!/usr/bin/env python3
"""Hardware-free checks for presentation values and shared-control boundaries."""
from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parents[1]
QML = ROOT / 'rust/qml'
# These files define colors rather than choosing an application control style.
COLOR_SOURCES = {
    'Theme.qml': 'application presentation values',
    'GuidePalette.qml': 'broadcast genre/time-band data colors',
    'SubtitleGlyph.qml': 'broadcast subtitle outline fallback',
    'SubtitleOverlay.qml': 'broadcast subtitle fallback and shadow',
    'DanmakuShadow.qml': 'rendered comment shadow',
    'DanmakuOverlay.qml': 'rendered comment shadow and diagnostic bounds',
}
SHARED_CONTROLS = {
    'ActionButton', 'IconAction', 'SettingsField', 'SettingsChoice',
    'ThemedSpinBox', 'ThemedSlider', 'ToggleSwitch', 'SegmentedControl',
}
LEXEMES = re.compile(r'(?P<string>"(?:\\.|[^"\\])*"|\'(?:\\.|[^\'\\])*\')|(?P<comment>//[^\n]*|/\*[\s\S]*?\*/)')
COLOR = re.compile(r'["\'](?:#[0-9a-fA-F]{3,8}|black|white)["\']')
# These compositions own media geometry or a settings row around a shared control.
CUSTOMIZATIONS = {
    ('LiveTimeline', 'ThemedSlider'): 'buffer and program ranges along the seek track',
    ('RecordingTimeline', 'ThemedSlider'): 'program ranges along the seek track',
    ('SettingsToggle', 'ToggleSwitch'): 'label/description row around the unchanged switch',
}
LITERAL_VALUE = re.compile(r'\b(font\.pixelSize|radius|duration)\s*:\s*\d')
STRUCTURE = re.compile(r'(?P<open>\b[A-Za-z_]\w*(?:\.\w+)*\s*\{)|(?P<brace>[{}])|(?P<override>\b(?:background|contentItem)\s*:)')


def violations(path: Path, source: str) -> list[str]:
    errors = []

    def report(offset: int, message: str) -> None:
        line = source.count('\n', 0, offset) + 1
        errors.append(f'{path.name}:{line}: {message}')

    # Keep offsets/newlines stable; ignore comments, URLs and strings when
    # checking QML property assignments and object nesting.
    masked = list(source)
    for token in LEXEMES.finditer(source):
        if token.lastgroup == 'string' and path.name not in COLOR_SOURCES and COLOR.fullmatch(token[0]):
            report(token.start(), 'use a named Theme/GuidePalette color')
        for index in range(token.start(), token.end()):
            if masked[index] != '\n':
                masked[index] = ' '
    code = ''.join(masked)
    if path.name not in {'Theme.qml', 'SubtitleOverlay.qml'}:
        for match in LITERAL_VALUE.finditer(code):
            report(match.start(), f'use a named value for {match[1]}')
    stack = []
    for match in STRUCTURE.finditer(code):
        if match.lastgroup == 'open':
            stack.append(match[0].split('{')[0].strip().rsplit('.', 1)[-1])
        elif match[0] == '{':
            stack.append(None)
        elif match[0] == '}':
            if stack:
                stack.pop()
        elif (stack and stack[-1] in SHARED_CONTROLS and path.stem != stack[-1]
              and (path.stem, stack[-1]) not in CUSTOMIZATIONS):
            report(match.start(), f'{stack[-1]} owns its {match[0].split(":")[0].strip()}; select a variant or change the shared component')
    return errors


def main() -> int:
    paths = sorted(QML.rglob('*.qml'))
    errors = [error for path in paths if 'tests' not in path.relative_to(QML).parts
              for error in violations(path, path.read_text())]
    if errors:
        print('\n'.join(errors), file=sys.stderr)
        return 1
    print('UI style: shared colors, dimensions, motion and control boundaries passed')
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
