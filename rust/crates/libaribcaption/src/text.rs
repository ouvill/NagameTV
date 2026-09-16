//! ARIB STD-B24 SI strings, profile A initial designation (not UTF-8 or DVB text).
//! Unsupported DRCS/mosaics produce U+FFFD. Formatting controls are not rendered.
#[derive(Clone, Copy)]
enum Set {
    Kanji,
    Alpha,
    Hiragana,
    Katakana,
    JisKana,
    Unknown(usize),
}
impl Set {
    fn width(self) -> usize {
        match self {
            Self::Kanji => 2,
            Self::Unknown(n) => n,
            _ => 1,
        }
    }
    fn character(self, first: u8, second: u8) -> char {
        let set = match self {
            Self::Kanji => 0,
            Self::Alpha => 1,
            Self::Hiragana => 2,
            Self::Katakana => 3,
            Self::JisKana => 4,
            Self::Unknown(_) => return '\u{fffd}',
        };
        // SAFETY: the stateless lookup takes only values and independently checks
        // both character indices. It neither retains pointers nor allocates.
        char::from_u32(unsafe { crate::sys::aribcc_rs_character(set, first, second) })
            .unwrap_or('\u{fffd}')
    }
}

pub fn decode(bytes: &[u8]) -> String {
    let mut sets = [Set::Kanji, Set::Alpha, Set::Hiragana, Set::Katakana];
    let (mut gl, mut gr, mut single) = (0, 2, None);
    let mut result = String::new();
    let mut bytes = bytes.iter().copied();
    while let Some(byte) = bytes.next() {
        match byte {
            0x0e => gl = 1,
            0x0f => gl = 0,
            0x19 => single = Some(2),
            0x1d => single = Some(3),
            0x20 | 0xa0 => result.push(' '),
            0x0d => result.push('\n'),
            0x1b => match bytes.next() {
                Some(0x6e) => gl = 2,
                Some(0x6f) => gl = 3,
                Some(0x7e) => gr = 1,
                Some(0x7d) => gr = 2,
                Some(0x7c) => gr = 3,
                Some(first @ (0x24 | 0x28..=0x2b)) => {
                    let width = if first == 0x24 { 2 } else { 1 };
                    let Some(mut final_byte) = bytes.next() else {
                        break;
                    };
                    let slot = if first == 0x24 {
                        if (0x28..=0x2b).contains(&final_byte) {
                            let slot = (final_byte - 0x28) as usize;
                            let Some(value) = bytes.next() else { break };
                            final_byte = value;
                            slot
                        } else {
                            0
                        }
                    } else {
                        (first - 0x28) as usize
                    };
                    sets[slot] = if final_byte == 0x20 {
                        bytes.next();
                        Set::Unknown(width)
                    } else {
                        match (width, final_byte) {
                            (2, 0x42 | 0x39 | 0x3b) => Set::Kanji,
                            (1, 0x4a | 0x36) => Set::Alpha,
                            (1, 0x30 | 0x37) => Set::Hiragana,
                            (1, 0x31 | 0x38) => Set::Katakana,
                            (1, 0x49) => Set::JisKana,
                            _ => Set::Unknown(width),
                        }
                    };
                }
                _ => result.push('\u{fffd}'),
            },
            0x21..=0x7e | 0xa1..=0xfe => {
                let slot = single.take().unwrap_or(if byte < 0x80 { gl } else { gr });
                let set = sets[slot];
                let second = if set.width() == 2 {
                    bytes.next().unwrap_or(0) & 0x7f
                } else {
                    0
                };
                result.push(set.character(byte & 0x7f, second));
            }
            // Parameterized presentation controls. Do not mistake parameters for text.
            0x16 | 0x8b | 0x91 | 0x93 | 0x94 | 0x97 | 0x98 => {
                bytes.next();
            }
            0x1c => {
                bytes.next();
                bytes.next();
            }
            0x90 | 0x92 | 0x9d => {
                if bytes.next() == Some(0x20) {
                    bytes.next();
                }
            }
            0x9b => {
                for part in bytes.by_ref() {
                    if (0x40..=0x7e).contains(&part) {
                        break;
                    }
                }
            }
            0x95 => {
                // Macro definitions have no textual representation.
                result.push('\u{fffd}');
                break;
            }
            _ => {}
        }
    }
    result.trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn japanese_designations_shifts_and_truncation() {
        assert_eq!(decode(&[0x46, 0x7c, 0x4b, 0x5c, 0x38, 0x6c]), "日本語");
        assert_eq!(decode(b"\x0eNEWS \x19\x22\xa4"), "NEWS あい");
        assert_eq!(decode(b"\x1b\x28\x31\x22\x24"), "アイ");
        assert_eq!(decode(&[0x46]), "�");
        assert_eq!(decode(b"\x1b\x28\x20\x41\x21"), "�");
        assert_eq!(decode(&[0x7a, 0x56]), "🈑");
        for length in 0..6 {
            let _ = decode(&b"\x1b\x24\x28\x20\x40\x21"[..length]);
        }
    }
}
