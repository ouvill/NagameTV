//! Serialize borrowed schedules directly, without collecting every visible cell first.
use super::{
    guide::DayWindow,
    model::{Program, Snapshot},
    watch::Identity,
};
use crate::channels::Channel;
use serde::{Serialize, Serializer};

#[derive(Serialize)]
struct Cell<'a> {
    #[serde(flatten)]
    program: &'a Program,
    #[serde(rename = "watchKey", serialize_with = "Identity::serialize_key")]
    identity: Identity,
}
struct Programs<'a> {
    schedule: &'a [Program],
    endpoint: u64,
    window: DayWindow,
}
impl Serialize for Programs<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(
            self.schedule
                .iter()
                .filter(|p| self.window.overlaps(p.start_at, p.duration))
                .map(|program| Cell {
                    program,
                    identity: Identity::new(self.endpoint, program),
                }),
        )
    }
}
#[derive(Serialize)]
struct Column<'a> {
    index: usize,
    programs: Programs<'a>,
}
struct Grid<'a> {
    snapshot: &'a Snapshot,
    channels: &'a [Channel],
    window: DayWindow,
}
impl Serialize for Grid<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(
            self.channels
                .iter()
                .enumerate()
                .map(|(index, channel)| Column {
                    index,
                    programs: Programs {
                        schedule: self.snapshot.schedule(channel.broadcast),
                        endpoint: channel.id,
                        window: self.window,
                    },
                }),
        )
    }
}
pub(super) fn json(
    snapshot: &Snapshot,
    channels: &[Channel],
    window: DayWindow,
) -> Result<String, serde_json::Error> {
    // The output string and each temporary watch key still allocate; the nested
    // arrays themselves are streamed to Serde and never retained as extra Vecs.
    serde_json::to_string(&Grid {
        snapshot,
        channels,
        window,
    })
}
