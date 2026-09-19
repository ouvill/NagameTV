//! Preserve broadcaster-authored sections in API order, including unfamiliar headings.
use serde::{
    Deserialize, Deserializer, Serialize,
    de::{MapAccess, Visitor},
};
use std::fmt;

#[derive(Debug, Serialize)]
pub struct Section {
    heading: String,
    text: String,
}

#[derive(Debug, Serialize)]
#[serde(transparent)]
pub struct Extended(Box<[Section]>);

impl Extended {
    pub fn record_bytes(&self) -> usize {
        self.0.len() * std::mem::size_of::<Section>()
    }
    pub fn string_bytes(&self) -> usize {
        self.0
            .iter()
            .map(|section| section.heading.capacity() + section.text.capacity())
            .sum()
    }
}

impl<'de> Deserialize<'de> for Extended {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Sections;
        impl<'de> Visitor<'de> for Sections {
            type Value = Extended;
            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("an object of EPG headings and text")
            }
            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Extended, M::Error> {
                let mut sections = Vec::new();
                while let Some((heading, text)) = map.next_entry()? {
                    sections.push(Section { heading, text });
                }
                Ok(Extended(sections.into_boxed_slice()))
            }
        }
        deserializer.deserialize_map(Sections)
    }
}

// Strings preserve future API codec/resolution names instead of rejecting an EPG
// snapshot when a broadcaster adds a format the UI has not named yet.
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Video {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    encoding: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    resolution: Option<String>,
}

impl Video {
    pub fn string_bytes(&self) -> usize {
        self.encoding.as_ref().map_or(0, String::capacity)
            + self.resolution.as_ref().map_or(0, String::capacity)
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Series {
    pub name: Option<String>,
    pub episode: Option<u16>,
    pub last_episode: Option<u16>,
}

impl Series {
    pub fn string_bytes(&self) -> usize {
        self.name.as_ref().map_or(0, String::capacity)
    }
}
