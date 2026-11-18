use std::{
    collections::VecDeque,
    io::{self, Write},
    sync::Mutex,
};
use tracing_subscriber::{
    field::RecordFields,
    fmt::{FormatFields, MakeWriter, format},
};

const MAX_RECORDS: usize = 300;
const MAX_RECORD_BYTES: usize = 2048;
const TRUNCATED: &str = " [truncated]\n";

pub(super) static RECENT: Records = Records(Mutex::new(VecDeque::new()));

#[derive(Default)]
pub(super) struct Records(Mutex<VecDeque<String>>);

impl Records {
    pub(super) fn text(&self) -> String {
        self.0
            .lock()
            .expect("recent log lock")
            .iter()
            .map(String::as_str)
            .collect()
    }
}

pub(super) struct Capture<'a> {
    records: &'a Records,
    text: String,
    truncated: bool,
}

impl<'a> MakeWriter<'a> for &Records {
    type Writer = Capture<'a>;

    fn make_writer(&'a self) -> Self::Writer {
        Capture {
            records: self,
            text: String::new(),
            truncated: false,
        }
    }
}

impl Write for Capture<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if !self.truncated {
            let text = std::str::from_utf8(bytes).map_err(io::Error::other)?;
            let end = text.floor_char_boundary(MAX_RECORD_BYTES - self.text.len());
            self.text.push_str(&text[..end]);
            self.truncated = end < text.len();
        }
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Drop for Capture<'_> {
    fn drop(&mut self) {
        if self.text.is_empty() {
            return;
        }
        if self.truncated {
            self.text.truncate(
                self.text
                    .floor_char_boundary(MAX_RECORD_BYTES - TRUNCATED.len()),
            );
            self.text.push_str(TRUNCATED);
        }
        let mut records = self.records.0.lock().expect("recent log lock");
        if records.len() == MAX_RECORDS {
            records.pop_front();
        }
        records.push_back(std::mem::take(&mut self.text));
    }
}

// Each fmt layer caches span fields by formatter type. A distinct type keeps
// colored stderr span fields out of the plain-text snapshot.
#[derive(Default)]
pub(super) struct Fields(format::DefaultFields);

impl<'writer> FormatFields<'writer> for Fields {
    fn format_fields<R: RecordFields>(
        &self,
        writer: format::Writer<'writer>,
        fields: R,
    ) -> std::fmt::Result {
        self.0.format_fields(writer, fields)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing_subscriber::prelude::*;

    #[test]
    fn keeps_the_last_records_and_truncates_large_utf8_messages() -> io::Result<()> {
        let records = Records::default();
        let writer = &records;
        for index in 0..=MAX_RECORDS {
            writeln!(writer.make_writer(), "record {index}")?;
        }
        let payload = "あ".repeat(MAX_RECORD_BYTES);
        writer.make_writer().write_all(payload.as_bytes())?;
        let snapshot = records.text();
        assert_eq!(snapshot.lines().count(), MAX_RECORDS);
        assert!(snapshot.starts_with("record 2\n"));
        assert!(snapshot.contains(&format!("record {MAX_RECORDS}\n")));
        assert!(snapshot.ends_with(TRUNCATED));
        assert!(snapshot.lines().last().unwrap().len() < MAX_RECORD_BYTES);
        Ok(())
    }

    #[test]
    fn captures_qt_records_from_workers_without_ansi_or_filtered_events() {
        static RECORDS: Records = Records(Mutex::new(VecDeque::new()));
        let subscriber = tracing_subscriber::registry()
            .with(tracing_subscriber::EnvFilter::new("info"))
            .with(
                tracing_subscriber::fmt::layer()
                    .with_ansi(true)
                    .with_writer(io::sink),
            )
            .with(
                tracing_subscriber::fmt::layer()
                    .fmt_fields(Fields::default())
                    .with_ansi(false)
                    .with_writer(&RECORDS),
            );
        let dispatch = tracing::Dispatch::new(subscriber);
        std::thread::spawn(move || {
            tracing::dispatcher::with_default(&dispatch, || {
                let _span = tracing::info_span!("test", worker = 42).entered();
                crate::logging::record_qt(2, "test.category", "qt warning");
                crate::logging::record_qt(0, "test.category", "hidden debug message");
            });
        })
        .join()
        .unwrap();
        let text = RECORDS.text();
        assert!(
            text.contains("WARN") && text.contains("qt warning"),
            "{text}"
        );
        assert!(
            text.contains("test.category") && text.contains("worker=42"),
            "{text}"
        );
        assert!(
            !text.contains('\x1b') && !text.contains("hidden debug"),
            "{text}"
        );
    }
}
