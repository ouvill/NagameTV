use super::{Chapter, Error, List};
use gstreamer::{TocEntry, TocEntryType, TocRef, tags};
const MAX_NODES: usize = 1024;
const MAX_DEPTH: usize = 8;

pub(super) fn parse(toc: &TocRef) -> Result<List, Error> {
    let mut chapters = Vec::new();
    let mut remaining = MAX_NODES;
    visit(toc.entries(), 0, &mut remaining, &mut chapters)?;
    List::new(chapters)
}
fn visit(
    entries: Vec<TocEntry>,
    depth: usize,
    remaining: &mut usize,
    chapters: &mut Vec<Chapter>,
) -> Result<(), Error> {
    if depth > MAX_DEPTH || entries.len() > *remaining {
        return Err(Error::Limit);
    }
    *remaining -= entries.len();
    let mut edition_seen = false;
    for entry in entries {
        if entry.entry_type() == TocEntryType::Edition {
            // Alternative editions describe different timelines; use the first one.
            if edition_seen {
                continue;
            }
            edition_seen = true;
        }
        if entry.entry_type() == TocEntryType::Chapter
            && let Some((start, _)) = entry.start_stop_times()
        {
            if start < 0 {
                return Err(Error::Invalid("negative TOC chapter time"));
            }
            let name = entry
                .tags()
                .and_then(|tags| tags.get::<tags::Title>().map(|name| name.get().to_owned()))
                .unwrap_or_default();
            chapters.push(Chapter::new(start as u64 / 1_000_000, name)?);
            if chapters.len() > super::MAX_CHAPTERS {
                return Err(Error::Limit);
            }
        }
        visit(entry.sub_entries(), depth + 1, remaining, chapters)?;
    }
    Ok(())
}
