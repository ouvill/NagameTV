//! One playback's native stream catalog and confirmed selection; no stream history.
use gstreamer::{self as gst, prelude::*};
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("音声トラックが更新されています。もう一度選択してください")]
    Unavailable,
    #[error("音声切り替え要求が拒否されました")]
    Rejected,
}

#[derive(Serialize)]
pub struct Track {
    pub id: String,
    pub component_tag: Option<u8>,
    pub language: String,
    pub title: String,
    pub selected: bool,
}

#[derive(Default)]
pub struct Streams {
    collection: Option<gst::StreamCollection>,
    selected: Vec<gst::glib::GString>,
    requested: Option<String>,
    failure: Option<Error>,
    components: super::audio_components::Components,
}

impl Streams {
    pub fn for_service(service: Option<u16>) -> Self {
        Self {
            components: super::audio_components::Components::for_service(service),
            ..Self::default()
        }
    }
    /// Project only on request, rather than copying stream tags on each bus poll.
    pub fn tracks(&self) -> Vec<Track> {
        self.collection
            .iter()
            .flat_map(|collection| collection.iter())
            .filter(|stream| stream.stream_type().contains(gst::StreamType::AUDIO))
            .filter_map(|stream| {
                let id = stream.stream_id()?;
                let tags = stream.tags();
                Some(Track {
                    selected: self.selected.iter().any(|selected| selected == &id),
                    component_tag: self.components.tag_for_stream(id.as_str()),
                    id: id.to_string(),
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

    fn selection(&self, audio_id: &str) -> Result<Vec<gst::glib::GString>, Error> {
        let collection = self.collection.as_ref().ok_or(Error::Unavailable)?;
        let audio = collection
            .iter()
            .find(|stream| {
                stream.stream_type().contains(gst::StreamType::AUDIO)
                    && stream.stream_id().as_deref() == Some(audio_id)
            })
            .and_then(|stream| stream.stream_id())
            .ok_or(Error::Unavailable)?;
        let mut ids = Vec::new();
        let mut video = None;
        let mut selected_video = false;
        for stream in collection.iter() {
            if stream.stream_type().contains(gst::StreamType::AUDIO) {
                continue;
            }
            let Some(id) = stream.stream_id() else {
                continue;
            };
            let is_video = stream.stream_type().contains(gst::StreamType::VIDEO);
            if self.selected.contains(&id) {
                selected_video |= is_video;
                ids.push(id);
            } else if is_video && video.is_none() {
                video = Some(id);
            }
        }
        // Before the first confirmation, retain a video stream as main does.
        if !selected_video && let Some(video) = video {
            ids.push(video);
        }
        ids.push(audio);
        Ok(ids)
    }

    pub fn failure(&self) -> Option<Error> {
        self.failure
    }

    pub fn select(&mut self, player: &gst::Element, audio_id: &str) -> Result<(), Error> {
        let result = self.request(player, audio_id);
        self.failure = result.as_ref().err().copied();
        result
    }

    fn request(&mut self, player: &gst::Element, audio_id: &str) -> Result<(), Error> {
        let ids = self.selection(audio_id)?;
        if !player.send_event(gst::event::SelectStreams::new(
            ids.iter().map(|id| id.as_str()),
        )) {
            return Err(Error::Rejected);
        }
        // A successful send is a request, not confirmation of the selected track.
        self.requested = Some(audio_id.to_owned());
        Ok(())
    }

    pub fn observe(
        &mut self,
        player: &gst::Element,
        message: &gst::MessageRef,
    ) -> Result<(), Error> {
        if let Err(error) = self.components.observe(message) {
            tracing::debug!(%error, "Audio PMT rejected");
        }
        match message.view() {
            gst::MessageView::StreamCollection(message) => {
                self.collection = Some(message.stream_collection());
                self.selected.retain(|id| {
                    self.collection.as_ref().is_some_and(|collection| {
                        collection
                            .iter()
                            .any(|stream| stream.stream_id().as_ref() == Some(id))
                    })
                });
                // Collections can change within a program. Reapply the explicit choice
                // by identity, never by the position of a previous menu row.
                if let Some(requested) = self.requested.take() {
                    self.select(player, &requested)?;
                }
            }
            gst::MessageView::StreamsSelected(message) => {
                self.collection = Some(message.stream_collection());
                self.selected = message
                    .streams()
                    .filter_map(|stream| stream.stream_id())
                    .collect();
            }
            _ => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn catalog(ids: &[(&str, gst::StreamType)]) -> gst::StreamCollection {
        gst::StreamCollection::builder(None)
            .streams(ids.iter().map(|(id, kind)| {
                gst::Stream::new(Some(id), None, *kind, gst::StreamFlags::empty())
            }))
            .build()
    }

    #[test]
    fn selection_preserves_video_and_text_and_rejects_stale_or_non_audio_ids()
    -> Result<(), Box<dyn std::error::Error>> {
        gst::init()?;
        let streams = Streams {
            collection: Some(catalog(&[
                ("v", gst::StreamType::VIDEO),
                ("ja", gst::StreamType::AUDIO),
                ("en", gst::StreamType::AUDIO),
                ("t", gst::StreamType::TEXT),
            ])),
            selected: vec!["v".into(), "ja".into(), "t".into()],
            requested: None,
            failure: None,
            components: super::super::audio_components::Components::default(),
        };
        assert_eq!(streams.selection("en")?, ["v", "t", "en"]);
        assert!(matches!(streams.selection("gone"), Err(Error::Unavailable)));
        assert!(matches!(streams.selection("v"), Err(Error::Unavailable)));
        let tracks = streams.tracks();
        assert_eq!(tracks.len(), 2);
        assert!(tracks[0].selected);
        assert!(!tracks[1].selected);
        assert!(tracks.iter().all(|track| track.language.is_empty()));
        Ok(())
    }

    #[test]
    fn native_selection_event_preserves_other_streams_and_waits_for_confirmation()
    -> Result<(), Box<dyn std::error::Error>> {
        use std::sync::{Arc, Mutex};
        gst::init()?;
        // Explicit CPU test sink intercepts only events; it never renders media.
        let sink = gst::ElementFactory::make("fakesink").build()?;
        let observed = Arc::new(Mutex::new(Vec::<String>::new()));
        let output = observed.clone();
        let pad = sink.static_pad("sink").ok_or("missing test sink pad")?;
        pad.set_active(true)?;
        let probe = pad
            .add_probe(gst::PadProbeType::EVENT_UPSTREAM, move |_, info| {
                if let Some(event) = info.event()
                    && let gst::EventView::SelectStreams(event) = event.view()
                    && let Ok(mut ids) = output.lock()
                {
                    *ids = event.streams().iter().map(|id| id.to_string()).collect();
                    return gst::PadProbeReturn::Handled;
                }
                gst::PadProbeReturn::Ok
            })
            .ok_or("missing event probe")?;
        let collection = catalog(&[
            ("v", gst::StreamType::VIDEO),
            ("ja", gst::StreamType::AUDIO),
            ("en", gst::StreamType::AUDIO),
            ("t", gst::StreamType::TEXT),
        ]);
        let mut streams = Streams {
            collection: Some(collection.clone()),
            selected: vec!["v".into(), "ja".into(), "t".into()],
            requested: None,
            failure: None,
            components: super::super::audio_components::Components::default(),
        };
        streams.select(&sink, "en")?;
        assert_eq!(
            *observed.lock().map_err(|_| "poisoned test capture")?,
            ["v", "t", "en"]
        );
        assert!(streams.tracks()[0].selected);
        assert!(!streams.tracks()[1].selected);
        let confirmation = gst::message::StreamsSelected::builder(&collection)
            .streams(
                collection
                    .iter()
                    .filter(|stream| stream.stream_id().as_deref() != Some("ja")),
            )
            .build();
        streams.observe(&sink, &confirmation)?;
        assert!(!streams.tracks()[0].selected);
        assert!(streams.tracks()[1].selected);
        let replacement = catalog(&[("new", gst::StreamType::AUDIO)]);
        assert!(matches!(
            streams.observe(&sink, &gst::message::StreamCollection::new(&replacement)),
            Err(Error::Unavailable)
        ));
        assert!(streams.requested.is_none());
        assert_eq!(streams.failure(), Some(Error::Unavailable));
        streams.select(&sink, "new")?;
        assert_eq!(streams.failure(), None);
        assert!(streams.tracks().iter().all(|track| !track.selected));
        pad.remove_probe(probe);
        pad.set_active(false)?;
        Ok(())
    }

    #[test]
    fn replacement_and_reset_release_the_old_collection() -> Result<(), Box<dyn std::error::Error>>
    {
        gst::init()?;
        // No URI or state transition; this test does not open an output device.
        let player = gst::ElementFactory::make("playbin3").build()?;
        let mut streams = Streams::default();
        let old = catalog(&[("old", gst::StreamType::AUDIO)]);
        let weak = old.downgrade();
        streams.observe(&player, &gst::message::StreamCollection::new(&old))?;
        drop(old);
        assert!(weak.upgrade().is_some());
        let next = catalog(&[
            ("v", gst::StreamType::VIDEO),
            ("new", gst::StreamType::AUDIO),
        ]);
        streams.observe(&player, &gst::message::StreamCollection::new(&next))?;
        assert!(weak.upgrade().is_none());
        assert_eq!(streams.selection("new")?, ["v", "new"]);
        streams = Streams::default();
        assert!(streams.tracks().is_empty());
        assert!(matches!(streams.selection("new"), Err(Error::Unavailable)));
        Ok(())
    }
}

#[cfg(test)]
mod playback_tests;
