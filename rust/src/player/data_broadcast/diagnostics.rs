//! Classify the pinned web-bml's console messages at the presentation boundary.
//! Unknown diagnostics retain their severity; no message is discarded here.
use tracing::Level;

const BUNDLE_SOURCE: &str = "qrc:/qt/qml/MinimalViewer/assets/web-bml/bundle.js";

// WebEngineView.JavaScriptConsoleMessageLevel, not QtMsgType.
const CONSOLE_INFO: i32 = 0;
const CONSOLE_WARNING: i32 = 1;
const CONSOLE_ERROR: i32 = 2;

fn severity(level: i32, message: &str, source: &str) -> Level {
    // These exact diagnostics were checked against the pinned upstream code.
    // Limit the exceptions to our bundle: unrelated JavaScript with the same
    // text must not have its errors reclassified.
    if source == BUNDLE_SOURCE {
        if level == CONSOLE_ERROR
            && (message
                .starts_with("[browser] lockModuleOnMemoryEx: component does not exist in DII /")
                || message
                    == "[browser] unknown getBrowserSupport additionalinfo ARIB AITControlledAppEngineFunction"
                || message == "[browser] unknown getBrowserSupport pana dbgPrint")
        {
            // STD-B24 ModuleLocked status -2 is an ordinary absent-module
            // response. Capability queries likewise return 0 for unsupported
            // functions. Neither means that reception or execution stopped.
            return Level::DEBUG;
        }
        if level == CONSOLE_WARNING && message == "[interpreter] script execution timeout" {
            // The interpreter yields after 50 ms and resumes after 50 ms;
            // this is cooperative scheduling, not an execution failure.
            return Level::DEBUG;
        }
        if level == CONSOLE_ERROR && message.starts_with("[dom] unexpected streamStatus play ") {
            // The pinned MNG implementation assumes an initial stopped state,
            // but continues into updateAnimation() and starts playback. Keep
            // this compatibility limitation visible without claiming that the
            // operation stopped. Other DOM errors retain ERROR severity.
            return Level::WARN;
        }
    }
    match level {
        // WebEngine merges console.log/debug/info. Keep routine browser
        // chatter at DEBUG, as with Qt's default js category configuration.
        CONSOLE_INFO => Level::DEBUG,
        CONSOLE_WARNING => Level::WARN,
        _ => Level::ERROR,
    }
}

pub(crate) fn record_console(level: i32, message: &str, source: &str, line: i32) {
    match severity(level, message, source) {
        Level::DEBUG => tracing::debug!(target: "web_bml", source, line, "{message}"),
        Level::WARN => tracing::warn!(target: "web_bml", source, line, "{message}"),
        _ => tracing::error!(target: "web_bml", source, line, "{message}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logging::tests::capture_output;

    fn emit() {
        for (level, message, source) in [
            (
                CONSOLE_ERROR,
                "[browser] lockModuleOnMemoryEx: component does not exist in DII /40/1fff",
                BUNDLE_SOURCE,
            ),
            (
                CONSOLE_ERROR,
                "[browser] unknown getBrowserSupport additionalinfo ARIB AITControlledAppEngineFunction",
                BUNDLE_SOURCE,
            ),
            (
                CONSOLE_ERROR,
                "[browser] unknown getBrowserSupport pana dbgPrint",
                BUNDLE_SOURCE,
            ),
            (
                CONSOLE_WARNING,
                "[interpreter] script execution timeout",
                BUNDLE_SOURCE,
            ),
            (
                CONSOLE_ERROR,
                "[interpreter] unhandled error: test failure",
                BUNDLE_SOURCE,
            ),
            (
                CONSOLE_ERROR,
                "[nvram] originalNetworkId == null!",
                BUNDLE_SOURCE,
            ),
            (
                CONSOLE_ERROR,
                "[dom] unexpected streamStatus play ldmng.mng",
                BUNDLE_SOURCE,
            ),
            (
                CONSOLE_ERROR,
                "[browser] lockModuleOnMemoryEx: component does not exist in PMT /40/1fff",
                BUNDLE_SOURCE,
            ),
            (
                CONSOLE_ERROR,
                "[browser] lockModuleOnMemoryEx: component does not exist in DII /40/1fff",
                "file:///unrelated.js",
            ),
            (CONSOLE_WARNING, "Data broadcast warning", BUNDLE_SOURCE),
        ] {
            record_console(level, message, source, 17);
        }
    }

    #[test]
    fn expected_responses_are_available_at_debug_without_hiding_failures() {
        let (_, output) = capture_output("info", emit);
        assert!(
            !output.contains("AITControlledAppEngineFunction"),
            "{output}"
        );
        assert!(!output.contains("pana dbgPrint"), "{output}");
        assert!(!output.contains("script execution timeout"), "{output}");
        // Only the unrelated page's same-named diagnostic remains an error.
        assert_eq!(
            output.matches("does not exist in DII").count(),
            1,
            "{output}"
        );
        assert_eq!(output.matches("ERROR").count(), 4, "{output}");
        assert_eq!(output.matches("WARN").count(), 2, "{output}");
        assert!(output.contains("unhandled error: test failure"), "{output}");
        assert!(
            output.contains("source=\"file:///unrelated.js\""),
            "{output}"
        );
        assert!(output.contains("line=17"), "{output}");

        let (_, output) = capture_output("info,web_bml=debug", emit);
        assert_eq!(output.matches("DEBUG").count(), 4, "{output}");
        assert_eq!(output.matches("ERROR").count(), 4, "{output}");
        assert!(
            output.contains("AITControlledAppEngineFunction"),
            "{output}"
        );
        assert!(output.contains("script execution timeout"), "{output}");
    }
}
