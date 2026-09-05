use gst::prelude::*;
use gstreamer as gst;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{
    Arc,
    atomic::{AtomicI32, Ordering},
};

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

pub type ComponentMap = HashMap<u16, HashMap<u16, u8>>;

/// Accept only complete, current, CRC-valid single-section PMTs. Map ES PID to
/// component_tag within its service; never match EPG and stream array positions.
pub fn pmt_components(section: &[u8]) -> Option<(u16, HashMap<u16, u8>)> {
    if section.len() < 16
        || section[0] != 2
        || section[5] & 1 == 0
        || section[6] != 0
        || section[7] != 0
        || 3 + (((section[1] as usize & 15) << 8) | section[2] as usize) != section.len()
    {
        return None;
    }
    let mut crc = 0xffff_ffff_u32;
    for byte in section {
        crc ^= (*byte as u32) << 24;
        for _ in 0..8 {
            crc = if crc & 0x8000_0000 != 0 {
                (crc << 1) ^ 0x04c1_1db7
            } else {
                crc << 1
            };
        }
    }
    if crc != 0 {
        return None;
    }
    let end = section.len() - 4;
    let mut pos = 12 + (((section[10] as usize & 15) << 8) | section[11] as usize);
    let mut components = HashMap::new();
    while pos < end {
        if pos + 5 > end {
            return None;
        }
        let pid = ((section[pos + 1] as u16 & 31) << 8) | section[pos + 2] as u16;
        let next = pos + 5 + (((section[pos + 3] as usize & 15) << 8) | section[pos + 4] as usize);
        if next > end {
            return None;
        }
        let mut descriptor = pos + 5;
        while descriptor < next {
            if descriptor + 2 > next {
                return None;
            }
            let len = section[descriptor + 1] as usize;
            if descriptor + 2 + len > next {
                return None;
            }
            if section[descriptor] == 0x52 && len == 1 {
                components.insert(pid, section[descriptor + 2]);
            }
            descriptor += 2 + len;
        }
        pos = next;
    }
    if pos != end {
        return None;
    }
    Some((u16::from_be_bytes([section[3], section[4]]), components))
}

#[derive(Debug, Serialize)]
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

#[derive(Debug, Serialize)]
pub struct AudioTrack {
    pub id: String,
    pub language: String,
    pub title: String,
}

#[derive(Default)]
pub struct AudioStreams {
    streams: Vec<gst::Stream>,
    selected: Vec<String>,
    requested: Option<String>,
    pub error: String,
    pub program: Option<(u16, u64, Vec<ProgramAudio>)>,
    pub components: ComponentMap,
    pub choice: Option<String>,
}

impl AudioStreams {
    pub fn reset_choice(&mut self) {
        self.choice = None;
        self.requested = None;
        self.error.clear();
    }

    pub fn options(&self, mode: i32, channels: i32) -> Vec<AudioOption> {
        self.tracks()
            .iter()
            .enumerate()
            .flat_map(|(index, track)| {
                // Gst mpegtsbase uses <upstream collection ID>/<8-digit hex PID>.
                let metadata = self.program.as_ref().and_then(|(service, _, audios)| {
                    let suffix = track.id.rsplit_once('/')?.1;
                    if suffix.len() != 8 {
                        return None;
                    }
                    let pid = u16::from_str_radix(suffix, 16).ok()?;
                    let tag = self.components.get(service)?.get(&pid)?;
                    let mut matches = audios.iter().filter(|audio| audio.component_tag == *tag);
                    let audio = matches.next()?;
                    if matches.next().is_some() {
                        return None;
                    }
                    Some(audio)
                });
                let dual = metadata.is_some_and(|audio| audio.component_type & 0x1f == 2);
                let selected = self.selected.contains(&track.id);
                let modes: &[i32] = if dual { &[1, 2, 3] } else { &[0] };
                modes
                    .iter()
                    .map(move |&option_mode| {
                        let language = metadata
                            .and_then(|audio| audio.langs.get(if option_mode == 2 { 1 } else { 0 }))
                            .cloned()
                            .unwrap_or_else(|| {
                                if dual {
                                    String::new()
                                } else {
                                    track.language.clone()
                                }
                            });
                        let role = match option_mode {
                            1 => "main",
                            2 => "sub",
                            3 => "both",
                            _ if metadata.is_some_and(|audio| audio.is_main) => "main",
                            _ if metadata.is_some() => "sub",
                            _ => "",
                        };
                        AudioOption {
                            key: format!(
                                "{}:{}:{}",
                                track.id,
                                self.program.as_ref().map_or(0, |(_, start, _)| *start),
                                option_mode
                            ),
                            number: index + 1,
                            language,
                            role,
                            mode: option_mode,
                            selected: selected
                                && (mode == option_mode || (dual && mode == 0 && option_mode == 3)),
                            enabled: !selected || !dual || channels == 2,
                            track: index,
                            default: metadata.is_some_and(|audio| audio.is_main)
                                && option_mode <= 1,
                        }
                    })
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    pub fn tracks(&self) -> Vec<AudioTrack> {
        self.streams
            .iter()
            .filter(|stream| stream.stream_type().contains(gst::StreamType::AUDIO))
            .filter_map(|stream| {
                let tags = stream.tags();
                Some(AudioTrack {
                    id: stream.stream_id()?.to_string(),
                    language: tags
                        .as_ref()
                        .and_then(|tags| tags.get::<gst::tags::LanguageCode>())
                        .map(|tag| tag.get().to_owned())
                        .unwrap_or_default(),
                    title: tags
                        .as_ref()
                        .and_then(|tags| tags.get::<gst::tags::Title>())
                        .map(|tag| tag.get().to_owned())
                        .unwrap_or_default(),
                })
            })
            .collect()
    }

    pub fn selected_index(&self) -> i32 {
        self.tracks()
            .iter()
            .position(|track| self.selected.contains(&track.id))
            .map_or(-1, |index| index as i32)
    }

    /// Select one audio stream while retaining the selected video and other streams.
    pub fn selection(&self, index: i32) -> Option<Vec<String>> {
        let tracks = self.tracks();
        let audio = &tracks.get(usize::try_from(index).ok()?)?.id;
        let mut ids: Vec<String> = self
            .streams
            .iter()
            .filter(|stream| !stream.stream_type().contains(gst::StreamType::AUDIO))
            .filter_map(|stream| stream.stream_id().map(|id| id.to_string()))
            .filter(|id| self.selected.contains(id))
            .collect();
        if !self.streams.iter().any(|stream| {
            stream.stream_type().contains(gst::StreamType::VIDEO)
                && stream
                    .stream_id()
                    .is_some_and(|id| ids.iter().any(|selected| selected == id.as_str()))
        }) {
            if let Some(video) = self
                .streams
                .iter()
                .find(|stream| stream.stream_type().contains(gst::StreamType::VIDEO))
                .and_then(|stream| stream.stream_id())
            {
                ids.push(video.to_string());
            }
        }
        ids.push(audio.clone());
        Some(ids)
    }

    pub fn select(&mut self, playbin: &gst::Element, index: i32) -> bool {
        let Some(ids) = self.selection(index) else {
            self.error = "This audio track is no longer available. Choose a track again.".into();
            return false;
        };
        if !playbin.send_event(gst::event::SelectStreams::new(
            ids.iter().map(String::as_str),
        )) {
            self.error = "Could not switch audio tracks. Try again.".into();
            return false;
        }
        self.requested = self
            .tracks()
            .get(index as usize)
            .map(|track| track.id.clone());
        self.error.clear();
        true
    }

    pub fn observe(&mut self, playbin: &gst::Element, message: &gst::MessageRef) {
        match message.view() {
            gst::MessageView::StreamCollection(message) => {
                self.streams = message.stream_collection().iter().collect();
                self.selected.retain(|id| {
                    self.streams
                        .iter()
                        .any(|stream| stream.stream_id().as_deref() == Some(id.as_str()))
                });
                if let Some(requested) = self.requested.clone() {
                    if let Some(index) =
                        self.tracks().iter().position(|track| track.id == requested)
                    {
                        self.select(playbin, index as i32);
                    } else {
                        self.requested = None;
                    }
                }
            }
            gst::MessageView::StreamsSelected(message) => {
                self.streams = message.stream_collection().iter().collect();
                self.selected = message
                    .streams()
                    .filter_map(|stream| stream.stream_id().map(|id| id.to_string()))
                    .collect();
            }
            _ => {}
        }
    }
}

/// Route confirmed dual mono after decoding. Main/sub preserves independent
/// left/right channels; selecting either language duplicates it to both speakers.
#[derive(Clone, Default)]
pub struct AudioRouting {
    mode: Arc<AtomicI32>,
    channels: Arc<AtomicI32>,
}

impl AudioRouting {
    pub fn mode(&self) -> i32 {
        self.mode.load(Ordering::Relaxed)
    }
    pub fn channels(&self) -> i32 {
        self.channels.load(Ordering::Relaxed)
    }
    pub fn set_mode(&self, mode: i32) -> bool {
        if !(0..=3).contains(&mode) || (mode != 0 && self.channels() != 2) {
            return false;
        }
        self.mode.store(mode, Ordering::Relaxed);
        true
    }
    pub fn reset(&self) {
        self.mode.store(0, Ordering::Relaxed);
        self.channels.store(0, Ordering::Relaxed);
    }

    pub fn filter(&self) -> Result<gst::Bin, gst::glib::Error> {
        let filter = gst::parse::bin_from_description(
            r#"audioconvert ! capsfilter caps="audio/x-raw,format=F32LE,layout=interleaved""#,
            true,
        )?;
        let routing = self.clone();
        filter
            .static_pad("src")
            .ok_or_else(|| {
                gst::glib::Error::new(gst::CoreError::Pad, "Audio filter has no source pad")
            })?
            .add_probe(
                gst::PadProbeType::BUFFER | gst::PadProbeType::EVENT_DOWNSTREAM,
                move |_, info| {
                    if let Some(event) = info.event() {
                        match event.view() {
                            gst::EventView::Caps(event) => {
                                let channels = event
                                    .caps()
                                    .structure(0)
                                    .and_then(|caps| caps.get::<i32>("channels").ok())
                                    .unwrap_or(0);
                                routing.channels.store(channels, Ordering::Relaxed);
                                if channels != 2 {
                                    routing.mode.store(0, Ordering::Relaxed);
                                }
                            }
                            gst::EventView::StreamStart(_) => {
                                routing.mode.store(0, Ordering::Relaxed)
                            }
                            _ => {}
                        }
                    }
                    let mode = routing.mode();
                    if mode != 0 && routing.channels() == 2 {
                        if let Some(buffer) = info.buffer_mut() {
                            if let Ok(mut data) = buffer.make_mut().map_writable() {
                                route_stereo(data.as_mut_slice(), mode);
                            }
                        }
                    }
                    gst::PadProbeReturn::Ok
                },
            );
        Ok(filter)
    }
}

fn route_stereo(data: &mut [u8], mode: i32) {
    if !(1..=2).contains(&mode) {
        return;
    }
    for frame in data.chunks_exact_mut(8) {
        // chunks_exact_mut(8) guarantees two disjoint four-byte PCM samples.
        // Copy the original bytes to preserve every F32 bit pattern, including NaNs.
        let (left, right) = frame.split_at_mut(4);
        if mode == 1 {
            right.copy_from_slice(left);
        } else {
            left.copy_from_slice(right);
        }
    }
}

#[cfg(test)]
mod tests {
    // In tests, unwrap/expect assert successful setup or an expected result.
    // Failures intentionally fail the test; they are not assumed impossible IO.
    use super::*;

    #[test]
    fn pcm_routing_preserves_sample_bits_and_incomplete_frames() {
        // Include a NaN payload and negative zero: routing must copy samples
        // without interpreting them or touching an incomplete trailing frame.
        let original = [0x01, 0x00, 0xc0, 0x7f, 0x00, 0x00, 0x00, 0x80, 0xaa];
        for mode in [1, 2] {
            let mut data = original;
            super::route_stereo(&mut data, mode);
            let selected = if mode == 1 {
                &original[..4]
            } else {
                &original[4..8]
            };
            assert_eq!(&data[..4], selected);
            assert_eq!(&data[4..8], selected);
            assert_eq!(data[8], 0xaa);
        }
        for mode in [0, 3, -1] {
            let mut data = original;
            super::route_stereo(&mut data, mode);
            assert_eq!(data, original);
        }
    }

    fn streams() -> AudioStreams {
        let stream = |id, kind| gst::Stream::new(Some(id), None, kind, gst::StreamFlags::empty());
        let japanese = stream("a-ja", gst::StreamType::AUDIO);
        let mut tags = gst::TagList::new();
        tags.get_mut()
            .unwrap()
            .add::<gst::tags::LanguageCode>(&"ja", gst::TagMergeMode::Replace);
        japanese.set_tags(Some(&tags));
        AudioStreams {
            streams: vec![
                stream("v", gst::StreamType::VIDEO),
                japanese,
                stream("a-en", gst::StreamType::AUDIO),
                stream("t", gst::StreamType::TEXT),
            ],
            selected: vec!["v".into(), "a-ja".into(), "t".into()],
            ..AudioStreams::default()
        }
    }

    fn broadcast_streams() -> AudioStreams {
        let mut result = AudioStreams::default();
        result.streams = ["source/00000202", "source/00000201"]
            .iter()
            .map(|id| {
                gst::Stream::new(
                    Some(id),
                    None,
                    gst::StreamType::AUDIO,
                    gst::StreamFlags::empty(),
                )
            })
            .collect();
        result.selected = vec!["source/00000201".into()];
        result.program = Some((
            10,
            1000,
            serde_json::from_str(
                r#"[
            {"componentTag":16,"componentType":2,"isMain":true,"langs":["jpn","eng"]},
            {"componentTag":17,"componentType":3,"isMain":false,"langs":["eng"]}
        ]"#,
            )
            .unwrap(),
        ));
        result
            .components
            .insert(10, HashMap::from([(0x201, 16), (0x202, 17)]));
        result
    }

    #[test]
    fn matches_components_despite_reversed_stream_order() {
        gst::init().unwrap();
        let audio = broadcast_streams();
        let options = audio.options(2, 2);
        assert_eq!(options.len(), 4);
        assert_eq!(options[0].language, "eng");
        assert_eq!(options[0].mode, 0); // Stereo is never split into languages.
        assert_eq!(options[1].language, "jpn");
        assert!(options[1].default);
        assert_eq!(options[2].language, "eng");
        assert!(options[2].selected);
        assert_eq!(options[3].role, "both");
        assert!(!audio.options(0, 1)[1].enabled);
    }

    #[test]
    fn unknown_or_wrong_service_metadata_never_enables_dual_mono() {
        gst::init().unwrap();
        let mut audio = broadcast_streams();
        audio.components.clear();
        assert!(
            audio
                .options(0, 2)
                .iter()
                .all(|option| option.mode == 0 && option.language.is_empty())
        );
        audio.components.insert(11, HashMap::from([(0x201, 16)]));
        assert_eq!(audio.options(0, 2).len(), 2);
        audio.program = None;
        assert_eq!(audio.options(0, 2).len(), 2);
    }

    #[test]
    fn program_changes_invalidate_option_keys_and_duplicate_tags_are_not_guessed() {
        gst::init().unwrap();
        let mut audio = broadcast_streams();
        let key = audio.options(0, 2)[1].key.clone();
        let (_, start, metadata) = audio.program.as_mut().unwrap();
        *start += 1;
        metadata.push(metadata[0].clone());
        let options = audio.options(0, 2);
        assert_eq!(options.len(), 2);
        assert!(
            options
                .iter()
                .all(|option| option.key != key && option.mode == 0)
        );
    }

    #[test]
    fn reads_pmt_identifiers_and_rejects_corruption_and_future_tables() {
        fn with_crc(mut data: Vec<u8>) -> Vec<u8> {
            let len = data.len() + 1;
            data[1] = 0xb0 | ((len >> 8) as u8);
            data[2] = len as u8;
            let mut crc = 0xffff_ffff_u32;
            for byte in &data {
                crc ^= (*byte as u32) << 24;
                for _ in 0..8 {
                    crc = if crc & 0x8000_0000 != 0 {
                        (crc << 1) ^ 0x04c1_1db7
                    } else {
                        crc << 1
                    };
                }
            }
            data.extend_from_slice(&crc.to_be_bytes());
            data
        }
        let body = vec![
            2, 0, 0, 0, 10, 0xc1, 0, 0, 0xe2, 0, 0xf0, 0, 0x0f, 0xe2, 1, 0xf0, 3, 0x52, 1, 16,
            0x0f, 0xe2, 2, 0xf0, 3, 0x52, 1, 17,
        ];
        let section = with_crc(body.clone());
        assert_eq!(
            pmt_components(&section),
            Some((10, HashMap::from([(0x201, 16), (0x202, 17)])))
        );
        let mut corrupt = section.clone();
        corrupt[19] ^= 1;
        assert!(pmt_components(&corrupt).is_none());
        let mut future = body.clone();
        future[5] &= !1;
        assert!(pmt_components(&with_crc(future)).is_none());
        let mut truncated_descriptor = body;
        truncated_descriptor[18] = 10;
        assert!(pmt_components(&with_crc(truncated_descriptor)).is_none());
        assert!(pmt_components(&section[..section.len() - 1]).is_none());
    }

    #[test]
    fn selecting_audio_retains_video_and_other_selected_streams() {
        gst::init().unwrap();
        let streams = streams();
        assert_eq!(streams.tracks()[0].language, "ja");
        assert_eq!(streams.selected_index(), 0);
        assert_eq!(streams.selection(1).unwrap(), ["v", "t", "a-en"]);
        assert!(streams.selection(-1).is_none());
        assert!(streams.selection(2).is_none());
    }

    #[test]
    fn confirms_selection_and_handles_program_stream_changes() {
        gst::init().unwrap();
        let playbin = gst::ElementFactory::make("playbin3").build().unwrap();
        let mut streams = streams();
        let collection = gst::StreamCollection::builder(None)
            .streams(streams.streams.clone())
            .build();
        let message = gst::message::StreamsSelected::builder(&collection)
            .streams([&streams.streams[0], &streams.streams[2]])
            .build();
        streams.observe(&playbin, &message);
        assert_eq!(streams.selected_index(), 1);
        let collection = gst::StreamCollection::builder(None)
            .streams([streams.streams[0].clone(), streams.streams[1].clone()])
            .build();
        streams.observe(&playbin, &gst::message::StreamCollection::new(&collection));
        assert_eq!(streams.tracks().len(), 1);
        assert_eq!(streams.selected_index(), -1);
        assert_eq!(streams.selection(0).unwrap(), ["v", "a-ja"]);
    }

    struct PipelineGuard(gst::Pipeline);
    impl Drop for PipelineGuard {
        fn drop(&mut self) {
            let _ = self.0.set_state(gst::State::Null);
        }
    }

    #[test]
    fn routes_pcm_through_the_production_filter_without_audio_devices() {
        gst::init().unwrap();
        let routing = AudioRouting::default();
        let pipeline = PipelineGuard(gst::Pipeline::new());
        let caps = gst::Caps::builder("audio/x-raw")
            .field("format", "F32LE")
            .field("layout", "interleaved")
            .field("rate", 48000_i32)
            .field("channels", 2_i32)
            .build();
        let source = gst::ElementFactory::make("appsrc")
            .property("caps", &caps)
            .build()
            .unwrap();
        let filter = routing.filter().unwrap();
        let sink = gst::ElementFactory::make("appsink")
            .property("sync", false)
            .property("async", false)
            .build()
            .unwrap();
        pipeline
            .0
            .add_many([&source, filter.upcast_ref(), &sink])
            .unwrap();
        gst::Element::link_many([&source, filter.upcast_ref(), &sink]).unwrap();
        pipeline.0.set_state(gst::State::Playing).unwrap();
        for (mode, expected) in [
            (0, [0.25, -0.75]),
            (1, [0.25, 0.25]),
            (2, [-0.75, -0.75]),
            (3, [0.25, -0.75]),
            (0, [0.25, -0.75]),
        ] {
            assert!(routing.set_mode(mode));
            let bytes: Vec<u8> = [0.25_f32, -0.75]
                .into_iter()
                .flat_map(f32::to_le_bytes)
                .collect();
            assert_eq!(
                source.emit_by_name::<gst::FlowReturn>(
                    "push-buffer",
                    &[&gst::Buffer::from_mut_slice(bytes)]
                ),
                gst::FlowReturn::Ok
            );
            let sample = sink
                .emit_by_name::<Option<gst::Sample>>("try-pull-sample", &[&2_000_000_000_u64])
                .expect("audio filter did not output PCM");
            let data = sample.buffer().unwrap().map_readable().unwrap();
            let actual: Vec<f32> = data
                .as_slice()
                .chunks_exact(4)
                .map(|value| f32::from_le_bytes(value.try_into().unwrap()))
                .collect();
            assert_eq!(actual, expected, "mode {mode}");
        }
        assert!(!routing.set_mode(4));
        // Mono input resets routing, rather than muting or misinterpreting it.
        let caps = gst::Caps::builder("audio/x-raw")
            .field("format", "F32LE")
            .field("layout", "interleaved")
            .field("rate", 48000_i32)
            .field("channels", 1_i32)
            .build();
        assert!(routing.set_mode(2));
        source.set_property("caps", &caps);
        let _ = source.emit_by_name::<gst::FlowReturn>(
            "push-buffer",
            &[&gst::Buffer::from_mut_slice(0.5_f32.to_le_bytes().to_vec())],
        );
        let sample = sink
            .emit_by_name::<Option<gst::Sample>>("try-pull-sample", &[&2_000_000_000_u64])
            .unwrap();
        assert_eq!(
            sample.buffer().unwrap().map_readable().unwrap().as_slice(),
            0.5_f32.to_le_bytes()
        );
        assert_eq!(routing.mode(), 0);
        assert_eq!(routing.channels(), 1);
        assert!(!routing.set_mode(1));
    }
}
