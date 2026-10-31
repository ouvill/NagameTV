//! Read only top-level atom headers and one bounded moov; never traverse mdat bytes.
use super::{Chapter, Error, List, MAX_CHAPTERS};
use std::{
    io::{Read, Seek, SeekFrom},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

const MAX_MOOV_BYTES: u64 = 16 * 1024 * 1024;
const MAX_TOP_LEVEL_BOXES: usize = 64;
const MAX_CHILD_BOXES: usize = 1024;
const READ_CHUNK_BYTES: usize = 64 * 1024;
const DISCOVERY_TIMEOUT: Duration = Duration::from_secs(10);
const TICKS_PER_MS: u64 = 10_000;

fn check(cancelled: &AtomicBool, deadline: Instant) -> Result<(), Error> {
    if cancelled.load(Ordering::Acquire) {
        return Err(Error::Cancelled);
    }
    if Instant::now() >= deadline {
        return Err(Error::TimedOut);
    }
    Ok(())
}
fn read(
    reader: &mut impl Read,
    bytes: &mut [u8],
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<(), Error> {
    for chunk in bytes.chunks_mut(READ_CHUNK_BYTES) {
        check(cancelled, deadline)?;
        reader.read_exact(chunk)?;
    }
    Ok(())
}
fn header(bytes: &[u8], remaining: u64) -> Result<(u64, usize, [u8; 4]), Error> {
    if bytes.len() < 8 || remaining < 8 {
        return Err(Error::Invalid("truncated atom header"));
    }
    let size = u32::from_be_bytes(bytes[..4].try_into().expect("four bytes"));
    let (size, length) = match size {
        0 => (remaining, 8),
        1 => (
            u64::from_be_bytes(
                bytes
                    .get(8..16)
                    .ok_or(Error::Invalid("truncated extended atom"))?
                    .try_into()
                    .expect("eight bytes"),
            ),
            16,
        ),
        size => (u64::from(size), 8),
    };
    if size < length as u64 || size > remaining {
        return Err(Error::Invalid("atom outside parent"));
    }
    Ok((size, length, bytes[4..8].try_into().expect("four bytes")))
}

pub(super) fn discover(
    reader: &mut (impl Read + Seek),
    size: u64,
    cancelled: &AtomicBool,
) -> Result<List, Error> {
    let deadline = Instant::now() + DISCOVERY_TIMEOUT;
    let mut offset = 0;
    for _ in 0..MAX_TOP_LEVEL_BOXES {
        if offset == size {
            return Ok(List::default());
        }
        let remaining = size - offset;
        if remaining < 8 {
            return Err(Error::Invalid("truncated top-level atom"));
        }
        check(cancelled, deadline)?;
        reader.seek(SeekFrom::Start(offset))?;
        let mut bytes = [0; 16];
        read(reader, &mut bytes[..8], cancelled, deadline)?;
        if bytes[..4] == 1_u32.to_be_bytes() {
            if remaining < 16 {
                return Err(Error::Invalid("truncated extended atom"));
            }
            read(reader, &mut bytes[8..], cancelled, deadline)?;
        }
        let (length, header_length, kind) = header(&bytes, remaining)?;
        if kind == *b"moov" {
            if length > MAX_MOOV_BYTES {
                return Err(Error::Limit);
            }
            let mut movie = vec![0; length as usize - header_length];
            read(reader, &mut movie, cancelled, deadline)?;
            return movie_chapters(&movie);
        }
        offset += length;
    }
    if offset == size {
        Ok(List::default())
    } else {
        Err(Error::Limit)
    }
}

struct Atom<'a> {
    kind: [u8; 4],
    body: &'a [u8],
}
fn children<'a>(mut bytes: &'a [u8], budget: &mut usize) -> Result<Vec<Atom<'a>>, Error> {
    let mut result = Vec::new();
    while !bytes.is_empty() {
        if *budget == 0 {
            return Err(Error::Limit);
        }
        *budget -= 1;
        let (size, length, kind) = header(bytes, bytes.len() as u64)?;
        result.push(Atom {
            kind,
            body: &bytes[length..size as usize],
        });
        bytes = &bytes[size as usize..];
    }
    Ok(result)
}
fn movie_chapters(movie: &[u8]) -> Result<List, Error> {
    let mut budget = MAX_CHILD_BOXES;
    let mut result = None;
    for Atom { kind, body } in children(movie, &mut budget)? {
        if kind != *b"udta" {
            continue;
        }
        for Atom { kind, body } in children(body, &mut budget)? {
            if kind != *b"chpl" {
                continue;
            }
            if result.is_some() {
                return Err(Error::Invalid("duplicate chapter atom"));
            }
            result = Some(parse(body)?);
        }
    }
    Ok(result.unwrap_or_default())
}
pub(super) fn parse(bytes: &[u8]) -> Result<List, Error> {
    // Nero version 1 has four reserved bytes after the full-box version/flags.
    if bytes.len() < 9 || bytes[0] != 1 {
        return Err(Error::Invalid("unsupported or truncated chpl header"));
    }
    let count = usize::from(bytes[8]);
    if count > MAX_CHAPTERS {
        return Err(Error::Limit);
    }
    let mut bytes = &bytes[9..];
    let mut chapters = Vec::with_capacity(count);
    for _ in 0..count {
        let entry = bytes
            .get(..9)
            .ok_or(Error::Invalid("truncated chapter entry"))?;
        let ticks = u64::from_be_bytes(entry[..8].try_into().expect("eight bytes"));
        let length = usize::from(entry[8]);
        let name = bytes
            .get(9..9 + length)
            .ok_or(Error::Invalid("truncated chapter name"))?;
        chapters.push(Chapter::new(
            ticks / TICKS_PER_MS,
            std::str::from_utf8(name)?.to_owned(),
        )?);
        bytes = &bytes[9 + length..];
    }
    if !bytes.is_empty() {
        return Err(Error::Invalid("trailing chapter bytes"));
    }
    List::new(chapters)
}

#[cfg(test)]
pub(super) fn fixture() -> Vec<u8> {
    let mut bytes = vec![1, 0, 0, 0, 0, 0, 0, 0, 3];
    for (milliseconds, name) in [(0_u64, "CM?"), (10_110, "A90Sec"), (109_743, "CM")] {
        bytes.extend_from_slice(&(milliseconds * TICKS_PER_MS).to_be_bytes());
        bytes.push(name.len() as u8);
        bytes.extend_from_slice(name.as_bytes());
    }
    bytes
}
