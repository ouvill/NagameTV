use serde::{Deserialize, Serialize};
/// Mirakurun /api/programs audio_component_descriptor fields (ARIB STD-B10).
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProgramAudio {
    pub component_tag: u8,
    pub component_type: u8,
    pub is_main: bool,
    #[serde(default)]
    pub langs: Vec<String>,
}

/// Audio metadata owned by one playback programme, independent of the EPG snapshot.
#[derive(Clone, Debug, PartialEq)]
pub struct AudioProgram {
    pub service_id: u16,
    pub start_at: u64,
    pub audios: Vec<ProgramAudio>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AudioOption {
    pub key: String,
    pub number: usize,
    pub language: String,
    pub role: &'static str,
    pub mode: i32,
    pub selected: bool,
    pub enabled: bool,
    #[serde(skip)]
    pub track: usize,
    #[serde(skip)]
    pub default: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AudioTrack {
    pub id: String,
    pub language: String,
    pub title: String,
}
