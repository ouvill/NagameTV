//! systemd's stderr stream protocol. The native journal socket would require
//! separate access from the application and could bypass its unit's namespace.
use std::io::{self, Write};
use tracing::{Level, Metadata};
use tracing_subscriber::fmt::MakeWriter;

#[derive(Clone, Copy)]
pub(super) enum Output {
    Plain,
    Journal,
}

impl Output {
    pub(super) fn detect() -> io::Result<Self> {
        #[cfg(target_os = "linux")]
        {
            use std::fs::File;
            use std::os::{fd::AsFd, unix::fs::MetadataExt};

            let Some(expected) = std::env::var("JOURNAL_STREAM")
                .ok()
                .as_deref()
                .and_then(stream_identity)
            else {
                // Missing or invalid inherited metadata supplies no stream identity.
                return Ok(Self::Plain);
            };
            let descriptor = std::io::stderr().as_fd().try_clone_to_owned()?;
            let stderr = File::from(descriptor).metadata()?;
            if expected == (stderr.dev(), stderr.ino()) {
                return Ok(Self::Journal);
            }
        }
        Ok(Self::Plain)
    }
}

#[cfg(target_os = "linux")]
fn stream_identity(value: &str) -> Option<(u64, u64)> {
    let (device, inode) = value.split_once(':')?;
    // An invalid JOURNAL_STREAM must never activate the stream protocol.
    Some((device.parse().ok()?, inode.parse().ok()?))
}

pub(super) enum Writer {
    Plain(io::Stderr),
    Journal(&'static [u8]),
}

impl<'a> MakeWriter<'a> for Output {
    type Writer = Writer;

    fn make_writer(&'a self) -> Self::Writer {
        Writer::Plain(io::stderr())
    }

    fn make_writer_for(&'a self, metadata: &Metadata<'_>) -> Self::Writer {
        match self {
            Self::Plain => self.make_writer(),
            Self::Journal => Writer::Journal(priority(*metadata.level())),
        }
    }
}

fn priority(level: Level) -> &'static [u8] {
    match level {
        Level::ERROR => b"<3>",
        Level::WARN => b"<4>",
        Level::INFO => b"<6>",
        Level::DEBUG | Level::TRACE => b"<7>",
    }
}

// Stay below journald's default LineMax so long messages retain their
// priority even when broken into multiple stream records.
const MAX_LINE_BYTES: usize = 8 * 1024;

fn write_record(writer: &mut impl Write, priority: &[u8], bytes: &[u8]) -> io::Result<()> {
    let record = std::str::from_utf8(bytes).map_err(io::Error::other)?;
    // journald splits on both newline and NUL. Prefix every physical line so
    // continuation text gets the event's priority too.
    for mut line in record.split_terminator(['\n', '\0']) {
        while !line.is_empty() {
            let end = line.floor_char_boundary(MAX_LINE_BYTES - priority.len());
            writer.write_all(priority)?;
            writer.write_all(&line.as_bytes()[..end])?;
            writer.write_all(b"\n")?;
            line = &line[end..];
        }
    }
    Ok(())
}

impl Write for Writer {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.write_all(bytes)?;
        Ok(bytes.len())
    }

    fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
        match self {
            Self::Plain(stderr) => stderr.write_all(bytes),
            Self::Journal(priority) => {
                // A lock across the whole event keeps other logging threads
                // from interleaving their output between a prefix and its text.
                write_record(&mut io::stderr().lock(), priority, bytes)
            }
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        io::stderr().flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "linux")]
    #[test]
    fn stream_identity_requires_both_numeric_components() {
        assert_eq!(
            stream_identity("8:18446744073709551615"),
            Some((8, u64::MAX))
        );
        for invalid in ["", "8", "8:", ":123", "8:1:2", "8:x", "8:-1"] {
            assert_eq!(stream_identity(invalid), None, "{invalid}");
        }
    }

    #[test]
    fn priority_matches_the_systemd_stream_protocol() -> io::Result<()> {
        for (level, expected) in [
            (Level::ERROR, "<3>"),
            (Level::WARN, "<4>"),
            (Level::INFO, "<6>"),
            (Level::DEBUG, "<7>"),
            (Level::TRACE, "<7>"),
        ] {
            let mut output = Vec::new();
            write_record(&mut output, priority(level), b"first\nsecond\0third\n")?;
            assert_eq!(
                output,
                format!("{expected}first\n{expected}second\n{expected}third\n").into_bytes(),
            );
        }
        Ok(())
    }

    #[test]
    fn wraps_long_stream_lines_on_utf8_boundaries() -> io::Result<()> {
        let message = "あ".repeat(MAX_LINE_BYTES);
        let mut output = Vec::new();
        write_record(&mut output, priority(Level::ERROR), message.as_bytes())?;
        let output = std::str::from_utf8(&output).map_err(io::Error::other)?;
        assert!(output.lines().all(|line| line.len() <= MAX_LINE_BYTES));
        let recovered: String = output
            .lines()
            .map(|line| line.strip_prefix("<3>").unwrap())
            .collect();
        assert_eq!(recovered, message);
        Ok(())
    }
}
