use super::model::{SubtitleCell, SubtitleCue};
use libaribcaption::{Caption, Color, Decoder};

pub(super) struct AribDecoder(Decoder);

impl AribDecoder {
    pub(super) fn new() -> Option<Self> {
        Decoder::new().ok().map(Self)
    }

    pub(super) fn decode_pes(&mut self, pes: &[u8], pts_ms: i64) -> Option<SubtitleCue> {
        self.0.decode(pes, pts_ms).ok().flatten().map(to_cue)
    }
}

fn to_cue(caption: Caption) -> SubtitleCue {
    let mut cells = Vec::new();
    for region in caption.regions {
        for character in region.characters {
            if character.text.is_empty() {
                continue;
            }
            cells.push(SubtitleCell {
                text: character.text,
                x: character.x,
                y: character.y,
                width: character.section_width.max(1),
                height: character.section_height.max(1),
                glyph_width: (character.width as f32 * character.horizontal_scale).round() as i32,
                glyph_height: (character.height as f32 * character.vertical_scale).round() as i32,
                foreground: qml_color(character.foreground),
                background: qml_color(character.background),
                stroke: qml_color(character.stroke),
                bold: character.bold,
                italic: character.italic,
                underline: character.underline,
                stroked: character.stroked,
                ruby: region.ruby,
            });
        }
    }
    SubtitleCue {
        text: caption.text,
        // Keep the player's existing presentation policy outside the library.
        duration_ms: caption
            .duration_ms
            .filter(|duration| *duration > 0)
            .map_or(7000, |duration| duration as u64),
        clear_screen: caption.clear_screen,
        plane_width: caption.plane_width,
        plane_height: caption.plane_height,
        cells,
    }
}

fn qml_color(color: Color) -> String {
    format!(
        "#{:02x}{:02x}{:02x}{:02x}",
        color.alpha, color.red, color.green, color.blue
    )
}

#[cfg(test)]
#[path = "../../crates/libaribcaption/tests/fixtures/sample.rs"]
mod fixture;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_player_caption_layout_and_opacity() {
        let mut decoder = AribDecoder::new().unwrap();
        let cue = decoder.decode_pes(fixture::SAMPLE, 1234).unwrap();
        assert_eq!(cue.text, "♬〜");
        assert_eq!(cue.duration_ms, 7000);
        assert!(cue.clear_screen);
        assert_eq!((cue.plane_width, cue.plane_height), (960, 540));
        assert_eq!(cue.cells.len(), 2);
        let cell = &cue.cells[0];
        assert_eq!((cell.x, cell.y), (170, 449));
        assert_eq!((cell.width, cell.height), (40, 60));
        assert_eq!((cell.glyph_width, cell.glyph_height), (36, 36));
        assert_eq!(cell.foreground, "#ffffffff");
        assert_eq!(cell.background, "#80000000");
        assert_eq!(cell.stroke, "#00000000");
    }

    #[test]
    fn keeps_duration_fallback_in_player() {
        for (duration, expected) in [
            (None, 7000),
            (Some(0), 7000),
            (Some(-1), 7000),
            (Some(2500), 2500),
        ] {
            let caption = Caption {
                text: String::new(),
                pts_ms: 0,
                duration_ms: duration,
                clear_screen: true,
                plane_width: 960,
                plane_height: 540,
                regions: vec![],
            };
            assert_eq!(to_cue(caption).duration_ms, expected);
        }
    }
}
