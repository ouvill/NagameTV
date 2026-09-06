//! Only the first broad genre is needed by main's guide palette.
use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::{IgnoredAny, SeqAccess, Visitor},
};
use std::fmt;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum Genre {
    News = 0,
    Sports = 1,
    Information = 2,
    Drama = 3,
    Music = 4,
    Variety = 5,
    Film = 6,
    Animation = 7,
    Documentary = 8,
    Theater = 9,
    Education = 10,
    Welfare = 11,
    #[default]
    Unknown = 15,
}

impl From<u8> for Genre {
    fn from(value: u8) -> Self {
        match value {
            0 => Self::News,
            1 => Self::Sports,
            2 => Self::Information,
            3 => Self::Drama,
            4 => Self::Music,
            5 => Self::Variety,
            6 => Self::Film,
            7 => Self::Animation,
            8 => Self::Documentary,
            9 => Self::Theater,
            10 => Self::Education,
            11 => Self::Welfare,
            _ => Self::Unknown,
        }
    }
}
impl Serialize for Genre {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(*self as u8)
    }
}
impl<'de> Deserialize<'de> for Genre {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct FirstGenre;
        impl<'de> Visitor<'de> for FirstGenre {
            type Value = Genre;
            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a broadcast genre array")
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<Genre, A::Error> {
                #[derive(Deserialize)]
                struct Entry {
                    lv1: u8,
                }
                let genre = sequence
                    .next_element::<Entry>()?
                    .map_or(Genre::Unknown, |entry| Genre::from(entry.lv1));
                // Consume remaining values without allocating an unused Vec per program.
                while sequence.next_element::<IgnoredAny>()?.is_some() {}
                Ok(genre)
            }
        }
        deserializer.deserialize_seq(FirstGenre)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn first_category_and_unknowns_serialize_as_main_palette_indices()
    -> Result<(), Box<dyn std::error::Error>> {
        for value in 0..=u8::MAX {
            let input = format!(r#"[{{"lv1":{value},"lv2":0}},{{"lv1":0,"lv2":0}}]"#);
            let genre: Genre = serde_json::from_str(&input)?;
            assert_eq!(
                serde_json::to_value(genre)?,
                if value < 12 { value } else { 15 }
            );
        }
        assert_eq!(serde_json::from_str::<Genre>("[]")?, Genre::Unknown);
        for input in ["null", "{}", "[{}]", "[{\"lv1\":-1}]", "[{\"lv1\":256}]"] {
            assert!(serde_json::from_str::<Genre>(input).is_err());
        }
        Ok(())
    }
}
