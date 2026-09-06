//! Broadcast audio metadata and roles, independent of Qt, GStreamer and clocks.
use serde::{Deserialize, Serialize};

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
    pub fn kind(&self) -> Kind {
        if self.component_type & 0x1f == 2 {
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
    pub id: u64,
    pub start: u64,
    pub duration: u64,
    pub service: crate::channels::BroadcastService,
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
