import QtQuick
import QtQuick.Controls
import QtTest
import MinimalViewer
Item {
    ApplicationWindow {
        id: host
        visible: true; width: 700; height: 900
        QtObject {
            id: backend
            property real timeshift_bytes_per_second: 0
            property string timeshift_storage: "memory"
            property string timeshift_limits: JSON.stringify({memory_mib:256, filesystem_mib:2048, minutes:30, min_mib:16, max_mib:65536, min_minutes:1, max_minutes:180})
            property var requests: []
            property bool acceptChanges: true
            function configure_timeshift_options(storage, memory, files, minutes) {
                requests = requests.concat([[storage, memory, files, minutes]]);
                if (!acceptChanges) return false;
                let limits = JSON.parse(timeshift_limits);
                limits.memory_mib = memory; limits.filesystem_mib = files; limits.minutes = minutes;
                timeshift_limits = JSON.stringify(limits);
                timeshift_storage = storage;
                return true;
            }
        }
        TimeshiftSettings { id: settings; x: 20; width: 660; backend: backend }
        Component { id: extraSettings; TimeshiftSettings { width: 660 } }
        TestCase {
            name: "TimeshiftSettings"
            when: windowShown
            function init() {
                failOnWarning(/.*/);
                tryCompare(settings, "editState", TimeshiftSettings.Synced);
                backend.acceptChanges = true;
                backend.timeshift_storage = "memory";
                backend.timeshift_limits = JSON.stringify({memory_mib:256, filesystem_mib:2048, minutes:30, min_mib:16, max_mib:65536, min_minutes:1, max_minutes:180});
                backend.requests = []; backend.timeshift_bytes_per_second = 0;
                settings.saveError = "";
                settings.storageValue = "memory";
                findChild(settings, "timeshiftMemoryLimit").value = 256;
                findChild(settings, "timeshiftFileLimit").value = 2048;
                findChild(settings, "timeshiftMinutes").value = 30;
                findChild(settings, "timeshiftEnabled").checked = true;
                host.requestActivate(); tryCompare(host, "active", true);
                waitForRendering(settings);
            }
            function test_repeated_steps_are_coalesced() {
                const memory = findChild(settings, "timeshiftMemoryLimit");
                memory.forceActiveFocus();
                keyClick(Qt.Key_Up); keyClick(Qt.Key_Up); keyClick(Qt.Key_Up);
                compare(backend.requests.length, 0);
                tryCompare(backend, "requests", [["memory", 259, 2048, 30]]);
                compare(settings.editState, TimeshiftSettings.Synced);
                verify(!findChild(settings, "applyTimeshiftSettings"));
            }
            function test_storage_segments_show_only_selected_budget_and_preserve_edits() {
                const memory = findChild(settings, "timeshiftMemoryLimit");
                const files = findChild(settings, "timeshiftFileLimit");
                verify(memory.visible); verify(!files.visible);
                memory.value = 128;
                mouseClick(findChild(settings, "timeshift-filesystem"));
                compare(settings.storageValue, "filesystem");
                mouseClick(findChild(settings, "timeshift-filesystem"));
                verify(findChild(settings, "timeshift-filesystem").checked);
                verify(files.visible); verify(!memory.visible);
                files.value = 4096;
                verify(findChild(settings, "timeshiftStorageDescription").text.includes("deleted"));
                const storage = findChild(settings, "timeshiftStorage");
                storage.focusCurrent(); keyClick(Qt.Key_Left);
                compare(settings.storageValue, "memory"); compare(memory.value, 128);
                keyClick(Qt.Key_Right);
                compare(settings.storageValue, "filesystem"); compare(files.value, 4096);
                compare(backend.requests[backend.requests.length - 1], ["filesystem", 128, 4096, 30]);
            }
            function test_estimate_uses_rate_and_both_limits() {
                const memory = findChild(settings, "timeshiftMemoryLimit");
                const minutes = findChild(settings, "timeshiftMinutes");
                const mib = 1024 * 1024;
                memory.value = 64;
                verify(!settings.measured);
                fuzzyCompare(settings.estimateSeconds, 64 * mib / (20 * 1000000 / 8), 0.001);
                backend.timeshift_bytes_per_second = mib;
                verify(settings.measured); compare(settings.estimateSeconds, 64);
                minutes.value = 1; compare(settings.estimateSeconds, 60);
                minutes.value = 30;
                memory.value = 128; compare(settings.estimateSeconds, 128);
                mouseClick(findChild(settings, "timeshift-filesystem"));
                compare(settings.estimateSeconds, 30 * 60);
            }
            function test_capacity_text_entry_saves_on_focus_loss() {
                const memory = findChild(settings, "timeshiftMemoryLimit");
                memory.contentItem.forceActiveFocus();
                keyClick(Qt.Key_A, Qt.ControlModifier);
                keyClick(Qt.Key_6); keyClick(Qt.Key_4); keyClick(Qt.Key_Tab);
                compare(memory.value, 64);
                tryCompare(backend, "requests", [["memory", 64, 2048, 30]]);
            }
            function test_failed_storage_change_restores_saved_values() {
                backend.acceptChanges = false;
                mouseClick(findChild(settings, "timeshift-filesystem"));
                compare(settings.storageValue, "memory");
                verify(findChild(settings, "timeshiftSaveError").visible);
                compare(backend.timeshift_storage, "memory");
            }
            function test_enter_saves_numeric_input() {
                const minutes = findChild(settings, "timeshiftMinutes");
                minutes.contentItem.forceActiveFocus();
                keyClick(Qt.Key_A, Qt.ControlModifier); keyClick(Qt.Key_5);
                compare(backend.requests.length, 0);
                keyClick(Qt.Key_Return);
                tryCompare(backend, "requests", [["memory", 256, 2048, 5]]);
            }
            function test_programmatic_updates_do_not_write_back() {
                backend.timeshift_limits = JSON.stringify({memory_mib:128, filesystem_mib:512, minutes:10, min_mib:16, max_mib:65536, min_minutes:1, max_minutes:180});
                compare(findChild(settings, "timeshiftMemoryLimit").value, 128);
                compare(findChild(settings, "timeshiftMinutes").value, 10);
                compare(backend.requests.length, 0);
            }
            function test_leaving_page_saves_unfinished_numeric_entry() {
                const page = extraSettings.createObject(host.contentItem, {backend: backend});
                verify(page);
                const memory = findChild(page, "timeshiftMemoryLimit");
                memory.contentItem.forceActiveFocus();
                keyClick(Qt.Key_A, Qt.ControlModifier); keyClick(Qt.Key_6); keyClick(Qt.Key_4);
                compare(backend.requests.length, 0);
                page.destroy();
                tryCompare(backend, "requests", [["memory", 64, 2048, 30]]);
            }
            function test_leaving_page_flushes_pending_steps() {
                const page = extraSettings.createObject(host.contentItem, {backend: backend});
                verify(page);
                const memory = findChild(page, "timeshiftMemoryLimit");
                memory.forceActiveFocus(); keyClick(Qt.Key_Up);
                compare(backend.requests.length, 0);
                page.destroy();
                tryCompare(backend, "requests", [["memory", 257, 2048, 30]]);
            }
            function test_disabled_timeshift_keeps_budgets_but_disables_editing() {
                mouseClick(findChild(settings, "timeshiftEnabled"));
                compare(findChild(settings, "timeshiftMemoryLimit").enabled, false);
                compare(backend.requests.length, 1);
                compare(backend.requests[0][0], "off");
            }
        }
    }
}
