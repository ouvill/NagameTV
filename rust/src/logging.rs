//! Application logging. Initialize once, before any worker threads are started.
use std::io::IsTerminal;
use tracing_subscriber::{EnvFilter, filter::LevelFilter};

pub fn init() {
    let filter = EnvFilter::builder()
        .with_default_directive(LevelFilter::INFO.into())
        .from_env_lossy();
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_ansi(std::io::stderr().is_terminal())
        .init();
}

/// QtMsgType values are mapped explicitly by the C++ bridge, not cast from its enum.
/// Qt remains responsible for aborting after a fatal message handler returns.
pub fn record_qt(level: u8, category: &str, message: &str) {
    match level {
        0 => tracing::debug!(target: "qt", category, "{message}"),
        1 => tracing::info!(target: "qt", category, "{message}"),
        2 => tracing::warn!(target: "qt", category, "{message}"),
        _ => tracing::error!(target: "qt", category, "{message}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    struct Buffer(Arc<Mutex<Vec<u8>>>);

    impl std::io::Write for Buffer {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn capture(filter: &str) -> String {
        let buffer = Buffer::default();
        let writer = buffer.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_env_filter(EnvFilter::new(filter))
            .without_time()
            .with_ansi(false)
            .with_writer(move || writer.clone())
            .finish();
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!("application started");
            for (level, message) in [
                (0, "qt-debug"),
                (1, "qt-info"),
                (2, "qt-warning"),
                (3, "qt-critical"),
                (4, "qt-fatal"),
            ] {
                record_qt(level, "test.category", message);
            }
        });
        String::from_utf8(buffer.0.lock().unwrap().clone()).unwrap()
    }

    #[test]
    fn qt_severity_and_category_survive_forwarding() {
        let output = capture("debug");
        for (level, message) in [
            ("DEBUG", "qt-debug"),
            ("INFO", "qt-info"),
            ("WARN", "qt-warning"),
            ("ERROR", "qt-critical"),
            ("ERROR", "qt-fatal"),
        ] {
            assert!(
                output.lines().any(|line| line.contains(level)
                    && line.contains("qt:")
                    && line.contains(message)
                    && line.contains("test.category")),
                "{output}"
            );
        }
    }

    #[test]
    fn filter_controls_application_and_qt_output() {
        let output = capture("info");
        assert!(output.contains("application started"));
        assert!(output.contains("qt-info"));
        assert!(!output.contains("qt-debug"));
        let output = capture("info,qt=error");
        assert!(output.contains("application started"));
        assert!(output.contains("qt-critical"));
        assert!(!output.contains("qt-warning"));
        assert!(!output.contains("qt-info"));
        assert!(capture("off").is_empty());
    }
}
