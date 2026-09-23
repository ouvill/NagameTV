import QtQuick
import QtQuick.Controls
import QtTest
import MinimalViewer

Item {
    ApplicationWindow {
        id: host
        visible: true; width: 700; height: 420
        QtObject {
            id: backend
            property string live_buffer_options: JSON.stringify({milliseconds:250, min_ms:1, max_ms:1000, default_ms:250})
            property var requests: []
            function configure_live_buffer(value) {
                requests = requests.concat([value]);
                const options = JSON.parse(live_buffer_options);
                if (value < options.min_ms || value > options.max_ms) return false;
                options.milliseconds = value;
                live_buffer_options = JSON.stringify(options);
                return true;
            }
        }
        LiveBufferSettings { id: settings; x: 20; y: 20; width: 660; backend: backend }
        Component { id: extraSettings; LiveBufferSettings { width: 660 } }
        TestCase {
            name: "LiveBufferSettings"
            when: windowShown
            function number() { return findChild(settings, "liveBufferMilliseconds"); }
            function enterValue(field, digits) {
                field.contentItem.forceActiveFocus();
                keyClick(Qt.Key_A, Qt.ControlModifier);
                for (let i = 0; i < digits.length; ++i) keyClick(digits.charCodeAt(i));
            }
            function init() {
                failOnWarning(/.*/);
                settings.finishEdit();
                settings.visible = true;
                number().locale = Qt.locale("en_US");
                backend.live_buffer_options = JSON.stringify({milliseconds:250, min_ms:1, max_ms:1000, default_ms:250});
                backend.requests = [];
                host.requestActivate(); tryCompare(host, "active", true);
                waitForRendering(settings);
            }
            function test_text_entry_commits_on_enter() {
                enterValue(number(), "150");
                compare(backend.requests.length, 0);
                keyClick(Qt.Key_Return);
                tryCompare(backend, "requests", [150]);
                compare(number().value, 150);
                number().forceActiveFocus();
                keyClick(Qt.Key_Up);
                tryCompare(backend, "requests", [150, 151]);
                compare(number().contentItem.text, "151");
            }
            function test_text_entry_commits_on_focus_loss() {
                enterValue(number(), "100");
                keyClick(Qt.Key_Tab);
                tryCompare(backend, "requests", [100]);
            }
            function test_repeated_steps_are_coalesced() {
                number().forceActiveFocus();
                keyClick(Qt.Key_Down); keyClick(Qt.Key_Down); keyClick(Qt.Key_Down);
                compare(backend.requests.length, 0);
                tryCompare(backend, "requests", [247]);
            }
            function test_hiding_page_commits_unfinished_entry_data() {
                return [
                    {tag: "plain", locale: "en_US", text: "120", value: 120},
                    {tag: "comma_grouping", locale: "en_US", text: "1,000", value: 1000},
                    {tag: "period_grouping", locale: "de_DE", text: "1.000", value: 1000}
                ];
            }
            function test_hiding_page_commits_unfinished_entry(data) {
                number().locale = Qt.locale(data.locale);
                enterValue(number(), data.text);
                compare(backend.requests.length, 0);
                compare(number().contentItem.text, data.text);
                settings.visible = false;
                tryCompare(backend, "requests", [data.value]);
            }
            function test_destroying_page_commits_unfinished_entry() {
                const page = extraSettings.createObject(host.contentItem, {backend: backend});
                verify(page);
                enterValue(findChild(page, "liveBufferMilliseconds"), "130");
                page.destroy();
                tryCompare(backend, "requests", [130]);
            }
            function test_saved_changes_do_not_write_back() {
                backend.live_buffer_options = JSON.stringify({milliseconds:175, min_ms:1, max_ms:1000, default_ms:250});
                compare(number().value, 175);
                compare(backend.requests.length, 0);
            }
            function test_reset_restores_default() {
                backend.configure_live_buffer(100);
                backend.requests = [];
                mouseClick(findChild(settings, "resetLiveBuffer"));
                compare(backend.requests, [250]);
                compare(number().value, 250);
            }
            function test_empty_entry_restores_saved_value() {
                enterValue(number(), "");
                keyClick(Qt.Key_Backspace);
                settings.visible = false;
                compare(backend.requests.length, 0);
                compare(number().value, 250);
                compare(number().contentItem.text, "250");
                settings.visible = true;
                number().forceActiveFocus();
                keyClick(Qt.Key_Up);
                tryCompare(backend, "requests", [251]);
                compare(number().contentItem.text, "251");
            }
        }
    }
}
