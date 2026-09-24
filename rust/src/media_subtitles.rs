//! Container subtitles. Native demuxers supply timed text; libass renders on
//! the presentation clock without touching decoded video or its GPU memory.
#[path = "media_subtitle_renderer.rs"]
mod renderer;
mod script;
use gstreamer::{self as gst, prelude::*};
use gstreamer_app::AppSink;
pub use script::Script;
use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{Arc, Mutex},
};

pub(crate) const MAX_TRACKS: usize = 128;
const MAX_PACKETS: usize = 4096;
const MAX_EVENTS: usize = 100_000;
pub(super) const HEADER: &str = "[Script Info]\nScriptType: v4.00+\nPlayResX: 1280\nPlayResY: 720\nWrapStyle: 0\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\nStyle: Default,sans-serif,42,&H00FFFFFF,&H00FFFFFF,&H00000000,&H80000000,0,0,0,0,100,100,0,0,1,2,1,2,32,32,28,1\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Could not read subtitles: {0}")]
    Read(#[from] std::io::Error),
    #[error("Select a valid SRT or ASS subtitle file")]
    Format,
    #[error("Save the subtitle file as UTF-8")]
    Encoding,
    #[error("Subtitle data exceeds the supported limit")]
    Capacity,
    #[error("Subtitle renderer: {0}")]
    Renderer(#[from] cxx::Exception),
    #[error("Subtitle sink: {0}")]
    Sink(#[from] gst::glib::BoolError),
    #[error("Subtitle track is no longer available")]
    Unavailable,
    #[error("Subtitle selection was rejected")]
    Rejected,
    #[error("Subtitle worker stopped unexpectedly")]
    Worker,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Track {
    pub id: String,
    pub title: String,
    pub selected: bool,
}

struct Packet {
    sample: gst::Sample,
}
#[derive(Default)]
struct Inbox {
    packets: VecDeque<Packet>,
    bytes: usize,
    generation: u64,
    overflow: bool,
}
impl Inbox {
    fn clear(&mut self) {
        self.packets.clear();
        self.bytes = 0;
        self.generation = self.generation.wrapping_add(1);
        self.overflow = false;
    }
    fn receive(&mut self, sample: gst::Sample) {
        let bytes = sample.buffer().map_or(0, |b| b.size());
        if self.packets.len() >= MAX_PACKETS || self.bytes.saturating_add(bytes) > script::MAX_BYTES
        {
            self.overflow = true;
            return;
        }
        self.bytes += bytes;
        self.packets.push_back(Packet { sample });
    }
}

enum Choice {
    Embedded(Option<External>),
    External(External),
}
impl Default for Choice {
    fn default() -> Self {
        Self::Embedded(None)
    }
}
struct External {
    script: Script,
    renderer: cxx::UniquePtr<renderer::ffi::Renderer>,
}
impl External {
    fn new(script: Script) -> Result<Self, Error> {
        let mut renderer = renderer::ffi::make_renderer()?;
        renderer.pin_mut().script(&script.bytes)?;
        Ok(Self { script, renderer })
    }
}
impl Choice {
    fn external(&self) -> Option<&External> {
        match self {
            Self::Embedded(external) => external.as_ref(),
            Self::External(external) => Some(external),
        }
    }
}
enum Load {
    Idle,
    Working {
        worker: std::thread::JoinHandle<Result<Script, Error>>,
        next: Option<PathBuf>,
    },
}

pub struct Frame {
    pub pixels: Vec<u8>,
    pub width: i32,
    pub height: i32,
}
pub struct Session {
    sink: AppSink,
    subscriptions: crate::features::subscriptions::Subscriptions,
    inbox: Arc<Mutex<Inbox>>,
    renderer: cxx::UniquePtr<renderer::ffi::Renderer>,
    generation: u64,
    caps: Option<gst::Caps>,
    last_buffer: Option<gst::Buffer>,
    events: usize,
    event_bytes: usize,
    choice: Choice,
    load: Load,
    clear: bool,
}
impl Session {
    pub fn start(playbin: &gst::Element, external: Option<Script>) -> Result<Self, Error> {
        // playsink retains its text chain through READY. Keep its native sink,
        // replacing only this stopped generation's observers and renderer.
        let sink = playbin.property::<Option<gst::Element>>("text-sink")
            .and_then(|sink| sink.downcast::<AppSink>().ok())
            .unwrap_or_else(|| AppSink::builder().sync(false).async_(false)
                .caps(&"text/x-raw,format=(string){utf8,pango-markup};application/x-ass;application/x-ssa;subpicture/x-pgs;subpicture/x-dvd;closedcaption/x-cea-608;closedcaption/x-cea-708".parse::<gst::Caps>().expect("subtitle caps"))
                .max_buffers(1).enable_last_sample(false).build());
        let mut renderer = renderer::ffi::make_renderer()?;
        renderer.pin_mut().header(HEADER.as_bytes());
        let choice = match external {
            Some(script) => Choice::External(External::new(script)?),
            None => Choice::default(),
        };
        let inbox = Arc::new(Mutex::new(Inbox::default()));
        let subscriptions = crate::features::subscriptions::Subscriptions::default();
        sink.set_callbacks(
            gstreamer_app::AppSinkCallbacks::builder()
                .new_sample(|sink| {
                    sink.pull_sample().map_err(|_| gst::FlowError::Eos)?;
                    Ok(gst::FlowSuccess::Ok)
                })
                .build(),
        );
        let input = inbox.clone();
        let pad = sink.static_pad("sink").expect("appsink pad");
        let probe = pad.add_probe(
            gst::PadProbeType::BUFFER
                | gst::PadProbeType::EVENT_DOWNSTREAM
                | gst::PadProbeType::EVENT_FLUSH,
            move |pad, info| {
                if info.event().is_some_and(|event| {
                    matches!(
                        event.view(),
                        gst::EventView::FlushStart(_) | gst::EventView::StreamStart(_)
                    )
                }) {
                    input.lock().unwrap_or_else(|e| e.into_inner()).clear();
                }
                // Observe before BaseSink's preroll wait: paused seeks also need the
                // cue covering the displayed frame, even while no sample is rendered.
                if let Some(buffer) = info.buffer()
                    && let Some(caps) = pad.current_caps()
                    && let Some(segment) = pad.sticky_event::<gst::event::Segment>(0)
                {
                    let sample = gst::Sample::builder()
                        .buffer(buffer)
                        .caps(&caps)
                        .segment(segment.segment())
                        .build();
                    input
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .receive(sample);
                }
                gst::PadProbeReturn::Ok
            },
        );
        subscriptions.probe(&pad, probe);
        playbin.set_property("text-sink", &sink);
        Ok(Self {
            sink,
            subscriptions,
            inbox,
            renderer,
            generation: 0,
            caps: None,
            last_buffer: None,
            events: 0,
            event_bytes: 0,
            choice,
            load: Load::Idle,
            clear: true,
        })
    }
    pub fn external(&self) -> Option<Script> {
        match &self.choice {
            Choice::External(external) => Some(external.script.clone()),
            Choice::Embedded(_) => None,
        }
    }
    pub fn tracks(&self, mut embedded: Vec<Track>) -> Vec<Track> {
        for track in &mut embedded {
            track.id.insert_str(0, "embedded:");
            if matches!(self.choice, Choice::External(_)) {
                track.selected = false;
            }
        }
        if let Some(external) = self.choice.external() {
            embedded.push(Track {
                id: "external".into(),
                title: external.script.name.clone(),
                selected: matches!(self.choice, Choice::External(_)),
            });
        }
        embedded
    }
    pub fn invalidate(&mut self) {
        self.clear = true;
    }
    pub fn loading(&self) -> bool {
        matches!(self.load, Load::Working { .. })
    }
    pub fn load(&mut self, path: PathBuf) -> Result<(), Error> {
        match &mut self.load {
            Load::Idle => self.load = Self::spawn(path)?,
            Load::Working { next, .. } => *next = Some(path),
        }
        Ok(())
    }
    fn spawn(path: PathBuf) -> Result<Load, Error> {
        Ok(Load::Working {
            worker: std::thread::Builder::new()
                .name("subtitle-file".into())
                .spawn(move || Script::load(&path))?,
            next: None,
        })
    }
    pub fn poll_load(&mut self) -> Option<Result<(), Error>> {
        if !matches!(&self.load, Load::Working { worker, .. } if worker.is_finished()) {
            return None;
        }
        let Load::Working { worker, next } = std::mem::replace(&mut self.load, Load::Idle) else {
            unreachable!()
        };
        let result = worker.join().unwrap_or(Err(Error::Worker));
        if let Some(path) = next {
            return match Self::spawn(path) {
                Ok(load) => {
                    self.load = load;
                    None
                }
                Err(error) => Some(Err(error)),
            };
        }
        Some(result.and_then(|script| {
            self.choice = Choice::External(External::new(script)?);
            self.clear = true;
            Ok(())
        }))
    }
    pub fn select_external(&mut self) -> Result<(), Error> {
        self.choice = match std::mem::take(&mut self.choice) {
            Choice::Embedded(Some(external)) | Choice::External(external) => {
                Choice::External(external)
            }
            Choice::Embedded(None) => return Err(Error::Unavailable),
        };
        self.clear = true;
        Ok(())
    }
    pub fn select_embedded(&mut self) -> Result<(), Error> {
        self.choice = match std::mem::take(&mut self.choice) {
            Choice::External(external) => Choice::Embedded(Some(external)),
            Choice::Embedded(external) => Choice::Embedded(external),
        };
        self.clear = true;
        Ok(())
    }
    fn reset_embedded(&mut self) -> Result<(), Error> {
        self.renderer.pin_mut().reset()?;
        self.renderer.pin_mut().header(HEADER.as_bytes());
        self.caps = None;
        self.last_buffer = None;
        self.events = 0;
        self.event_bytes = 0;
        self.clear = true;
        Ok(())
    }
    pub fn render(
        &mut self,
        position: gst::ClockTime,
        width: i32,
        height: i32,
    ) -> Result<Option<Frame>, Error> {
        let (generation, packets, overflow) = {
            let mut inbox = self.inbox.lock().unwrap_or_else(|e| e.into_inner());
            let packets = inbox.packets.drain(..).collect::<Vec<_>>();
            inbox.bytes = 0;
            (
                inbox.generation,
                packets,
                std::mem::take(&mut inbox.overflow),
            )
        };
        {
            if generation != self.generation {
                self.reset_embedded()?;
                self.generation = generation;
            }
            if overflow {
                return Err(Error::Capacity);
            }
            for packet in packets {
                self.ingest(&packet.sample)?;
            }
        }
        let clear = std::mem::take(&mut self.clear);
        let renderer = match &mut self.choice {
            Choice::Embedded(_) => &mut self.renderer,
            Choice::External(external) => &mut external.renderer,
        };
        let pixels = renderer
            .pin_mut()
            .render(position.mseconds() as i64, width, height, clear)?;
        if pixels.is_empty() && !clear {
            return Ok(None);
        }
        Ok(Some(Frame {
            pixels: if pixels.is_empty() {
                vec![0; width as usize * height as usize * 4]
            } else {
                pixels
            },
            width,
            height,
        }))
    }
    fn ingest(&mut self, sample: &gst::Sample) -> Result<(), Error> {
        let caps = sample.caps().ok_or(Error::Format)?;
        let structure = caps.structure(0).ok_or(Error::Format)?;
        if !supported_caps(caps) {
            return Ok(());
        }
        if self.caps.as_deref() != Some(caps) {
            self.reset_embedded()?;
            if let Ok(data) = structure.get::<gst::Buffer>("codec_data") {
                if data.size() > script::MAX_BYTES {
                    return Err(Error::Capacity);
                }
                self.renderer
                    .pin_mut()
                    .header(data.map_readable().map_err(|_| Error::Format)?.as_slice());
            }
            self.caps = Some(caps.to_owned());
        }
        let buffer = sample.buffer().ok_or(Error::Format)?;
        if self
            .last_buffer
            .as_ref()
            .is_some_and(|last| last.as_ptr() == buffer.as_ptr())
        {
            return Ok(());
        }
        self.last_buffer = Some(buffer.to_owned());
        let Some(segment) = sample
            .segment()
            .and_then(|segment| segment.downcast_ref::<gst::ClockTime>())
        else {
            return Ok(());
        };
        let Some(start) = buffer
            .pts()
            .and_then(|pts| segment.to_stream_time_full(pts))
            .and_then(|time| time.positive())
        else {
            return Ok(());
        };
        let Some(duration) = buffer.duration() else {
            return Ok(());
        };
        if duration.is_zero() {
            return Ok(());
        }
        let bytes = buffer.map_readable().map_err(|_| Error::Format)?;
        if self.events >= MAX_EVENTS
            || self.event_bytes.saturating_add(bytes.len()) > script::MAX_BYTES
        {
            return Err(Error::Capacity);
        }
        if matches!(
            structure.name().as_str(),
            "application/x-ass" | "application/x-ssa"
        ) {
            self.renderer.pin_mut().chunk(
                bytes.as_slice(),
                start.mseconds() as i64,
                duration.mseconds() as i64,
            );
        } else {
            let text = std::str::from_utf8(bytes.as_slice()).map_err(|_| Error::Encoding)?;
            let chunk = format!(
                "{},0,Default,,0,0,0,,{}",
                self.events,
                script::plain_text(text.trim_end_matches('\0'))
            );
            self.renderer.pin_mut().chunk(
                chunk.as_bytes(),
                start.mseconds() as i64,
                duration.mseconds() as i64,
            );
        }
        self.events += 1;
        self.event_bytes += bytes.len();
        Ok(())
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        self.subscriptions.close();
        self.sink
            .set_callbacks(gstreamer_app::AppSinkCallbacks::builder().build());
    }
}

pub(crate) fn supported_caps(caps: &gst::CapsRef) -> bool {
    caps.structure(0).is_some_and(|s| {
        matches!(
            s.name().as_str(),
            "text/x-raw" | "application/x-ass" | "application/x-ssa"
        )
    })
}

#[cfg(test)]
mod tests;
