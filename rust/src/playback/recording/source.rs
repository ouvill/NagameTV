//! Owned input locations and lazy independent cursors; construction performs no I/O.
use super::http;
use std::{
    io::{self, Read, Seek, SeekFrom},
    path::PathBuf,
    sync::{Arc, atomic::AtomicBool},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Location {
    Local(PathBuf),
    Http(url::Url),
}
impl Location {
    pub fn from_url(value: &str) -> Result<Self, super::Error> {
        let url = url::Url::parse(value)?;
        match url.scheme() {
            "file" => url
                .to_file_path()
                .map(Self::Local)
                .map_err(|_| super::Error::NotLocal),
            "http" | "https" if url.host_str().is_some() => Ok(Self::Http(url)),
            _ => Err(super::Error::InvalidUrl),
        }
    }
}

#[derive(Clone, Debug)]
pub(in crate::playback) enum Source {
    Local(PathBuf),
    Http(Arc<http::Verified>),
}
impl Source {
    pub(super) fn location(&self) -> Location {
        match self {
            Self::Local(path) => Location::Local(path.clone()),
            Self::Http(source) => Location::Http(source.url().clone()),
        }
    }
    pub fn reader(&self, cancelled: Arc<AtomicBool>) -> Reader {
        match self {
            Self::Local(path) => Reader::Pending(path.clone()),
            Self::Http(source) => Reader::Http(source.cursor(cancelled)),
        }
    }
}

pub(in crate::playback) enum Reader {
    Pending(PathBuf),
    Local(std::fs::File),
    Http(http::Cursor),
}
impl Reader {
    fn open(&mut self) -> io::Result<()> {
        if let Self::Pending(path) = self {
            *self = Self::Local(std::fs::File::open(path)?);
        }
        Ok(())
    }
}
impl Read for Reader {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.open()?;
        match self {
            Self::Local(file) => file.read(bytes),
            Self::Http(file) => file.read(bytes),
            Self::Pending(_) => unreachable!("opened file"),
        }
    }
}
impl Seek for Reader {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.open()?;
        match self {
            Self::Local(file) => file.seek(position),
            Self::Http(file) => file.seek(position),
            Self::Pending(_) => unreachable!("opened file"),
        }
    }
}
