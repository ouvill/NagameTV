//! Broadcast audio metadata and roles, independent of Qt, GStreamer and clocks.
use serde::{Deserialize, Serialize};

const AUDIO_MODE_MASK: u8 = 0x1f;
const DUAL_MONO_MODE: u8 = 0x02;
const COMPONENT_HEADER_BYTES: usize = 6;
const LANGUAGE_BYTES: usize = 3;
const COMPONENT_TYPE_OFFSET: usize = 1;
const COMPONENT_TAG_OFFSET: usize = 2;
const COMPONENT_FLAGS_OFFSET: usize = 5;
const MULTILINGUAL_FLAG: u8 = 0x80;
const MAIN_COMPONENT_FLAG: u8 = 0x40;

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Descriptor {
    pub component_tag: u8,
    component_type: u8,
    is_main: bool,
    #[serde(default)]
    pub langs: Box<[String]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    DualMono,
    Other,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    #[default]
    #[serde(rename = "")]
    Unknown,
    Main,
    Sub,
    Both,
}

impl Descriptor {
    /// ARIB STD-B10 Part 2, 6.2.26. Only a complete descriptor can supply roles.
    pub(crate) fn from_arib(body: &[u8]) -> Option<Self> {
        let header = body.get(..COMPONENT_HEADER_BYTES)?;
        let flags = header[COMPONENT_FLAGS_OFFSET];
        let languages = if flags & MULTILINGUAL_FLAG != 0 { 2 } else { 1 };
        let codes =
            body.get(COMPONENT_HEADER_BYTES..COMPONENT_HEADER_BYTES + languages * LANGUAGE_BYTES)?;
        let langs = codes
            .as_chunks::<LANGUAGE_BYTES>()
            .0
            .iter()
            .map(|code| {
                code.iter()
                    .all(u8::is_ascii_lowercase)
                    .then(|| String::from_utf8(code.to_vec()).ok())
                    .flatten()
            })
            .collect::<Option<Box<[_]>>>()?;
        Some(Self {
            component_tag: header[COMPONENT_TAG_OFFSET],
            component_type: header[COMPONENT_TYPE_OFFSET],
            is_main: flags & MAIN_COMPONENT_FLAG != 0,
            langs,
        })
    }
    pub fn kind(&self) -> Kind {
        if self.component_type & AUDIO_MODE_MASK == DUAL_MONO_MODE {
            Kind::DualMono
        } else {
            Kind::Other
        }
    }
    pub fn role(&self) -> Role {
        match self.kind() {
            Kind::DualMono => Role::Both,
            Kind::Other if self.is_main => Role::Main,
            Kind::Other => Role::Sub,
        }
    }
    pub fn is_main(&self) -> bool {
        self.is_main
    }
    pub fn heap_bytes(&self) -> usize {
        self.langs.len() * std::mem::size_of::<String>()
            + self.langs.iter().map(String::capacity).sum::<usize>()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct ProgramKey {
    pub source: u64,
    pub event: u16,
    pub start: Option<i64>,
    pub transport_stream: u16,
    pub service: crate::channels::BroadcastService,
}

/// Owned metadata from one input's position-scoped TS catalog. No wall-clock EPG.
pub(crate) struct Metadata {
    key: ProgramKey,
    descriptors: Box<[Descriptor]>,
}
impl Metadata {
    pub(crate) fn from_ts(source: u64, program: crate::transport::programs::Program) -> Self {
        Self {
            key: ProgramKey {
                source,
                event: program.event_id,
                start: program.start_at,
                transport_stream: program.transport_stream_id,
                service: crate::channels::BroadcastService {
                    network_id: program.network_id,
                    service_id: program.service_id,
                },
            },
            descriptors: program.audios,
        }
    }
    pub(crate) fn program(&self) -> Program<'_> {
        Program {
            key: self.key,
            descriptors: &self.descriptors,
        }
    }
}
#[derive(Clone, Copy)]
pub struct Program<'a> {
    pub key: ProgramKey,
    pub descriptors: &'a [Descriptor],
}

/// Ambiguous descriptors cannot assign a language or enable dual-mono routing.
pub fn matching(descriptors: &[Descriptor], tag: Option<u8>) -> Option<&Descriptor> {
    let tag = tag?;
    let mut matches = descriptors
        .iter()
        .filter(|audio| audio.component_tag == tag);
    let first = matches.next()?;
    matches.next().is_none().then_some(first)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn arib_components_require_complete_languages_and_preserve_roles()
    -> Result<(), Box<dyn std::error::Error>> {
        // MPEG-2 AAC dual mono, main component, Japanese + English.
        let dual = [
            0xf2, 0x02, 0x10, 0x0f, 0xff, 0xc7, b'j', b'p', b'n', b'e', b'n', b'g',
        ];
        let parsed = Descriptor::from_arib(&dual).ok_or("dual descriptor")?;
        assert_eq!(parsed.component_tag, 0x10);
        assert_eq!(parsed.kind(), Kind::DualMono);
        assert!(parsed.is_main());
        assert_eq!(&*parsed.langs, ["jpn", "eng"]);
        for end in 0..dual.len() {
            assert!(
                Descriptor::from_arib(&dual[..end]).is_none(),
                "truncation {end}"
            );
        }
        let mut single = dual[..COMPONENT_HEADER_BYTES + LANGUAGE_BYTES].to_vec();
        single[COMPONENT_FLAGS_OFFSET] &= !(MULTILINGUAL_FLAG | MAIN_COMPONENT_FLAG);
        single[COMPONENT_TYPE_OFFSET] = 0x03; // Stereo secondary component.
        let parsed = Descriptor::from_arib(&single).ok_or("single descriptor")?;
        assert_eq!(parsed.role(), Role::Sub);
        assert_eq!(&*parsed.langs, ["jpn"]);
        single[COMPONENT_HEADER_BYTES] = 0xff;
        assert!(Descriptor::from_arib(&single).is_none());
        Ok(())
    }
    #[test]
    fn explicit_tags_match_out_of_order_and_duplicates_remain_unknown()
    -> Result<(), Box<dyn std::error::Error>> {
        let descriptors: Vec<Descriptor> = serde_json::from_str(
            r#"[
            {"componentTag":17,"componentType":3,"isMain":false,"langs":["eng"]},
            {"componentTag":16,"componentType":2,"isMain":true,"langs":["jpn","eng"]}
        ]"#,
        )?;
        let dual = matching(&descriptors, Some(16)).ok_or("dual descriptor absent")?;
        assert_eq!(dual.kind(), Kind::DualMono);
        assert_eq!(dual.role(), Role::Both);
        assert_eq!(&*dual.langs, ["jpn", "eng"]);
        assert_eq!(
            matching(&descriptors, Some(17)).ok_or("sub absent")?.role(),
            Role::Sub
        );
        assert!(matching(&descriptors, None).is_none());
        assert!(matching(&descriptors, Some(99)).is_none());
        let duplicates: Vec<Descriptor> = serde_json::from_str(
            r#"[
            {"componentTag":16,"componentType":3,"isMain":false},
            {"componentTag":16,"componentType":2,"isMain":true}
        ]"#,
        )?;
        assert!(matching(&duplicates, Some(16)).is_none());
        let main: Descriptor =
            serde_json::from_str(r#"{"componentTag":0,"componentType":3,"isMain":true}"#)?;
        assert_eq!(main.kind(), Kind::Other);
        assert_eq!(main.role(), Role::Main);
        Ok(())
    }
}
