import QtQuick
import QtQuick.Controls
import QtTest
import ".."
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
            function configure_timeshift_options(storage, memory, files, minutes) { requests.push([storage, memory, files, minutes]); return true; }
        }
        TimeshiftSettings { id: settings; x: 20; width: 660; backend: backend }
        TestCase {
            name: "TimeshiftSettings"
            when: windowShown
            function init() {
                failOnWarning(/.*/);
                backend.requests = []; backend.timeshift_bytes_per_second = 0;
                settings.storageValue = "memory";
                findChild(settings, "timeshiftMemoryLimit").value = 256;
                findChild(settings, "timeshiftFileLimit").value = 2048;
                findChild(settings, "timeshiftMinutes").value = 30;
                findChild(settings, "timeshiftEnabled").checked = true;
                host.requestActivate(); tryCompare(host, "active", true);
                waitForRendering(settings);
            }
            function test_capacity_edit_is_applied_with_storage_and_time_as_one_request() {
                const memory = findChild(settings, "timeshiftMemoryLimit");
                memory.value = 64;
                findChild(settings, "timeshiftMinutes").value = 10;
                compare(backend.requests.length, 0);
                mouseClick(findChild(settings, "applyTimeshiftSettings"));
                compare(backend.requests.length, 1);
                compare(backend.requests[0], ["memory", 64, 2048, 10]);
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
                mouseClick(findChild(settings, "applyTimeshiftSettings"));
                compare(backend.requests[0], ["filesystem", 128, 4096, 30]);
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
            function test_capacity_text_entry_commits_before_apply() {
                const memory = findChild(settings, "timeshiftMemoryLimit");
                memory.contentItem.forceActiveFocus();
                keyClick(Qt.Key_A, Qt.ControlModifier);
                keyClick(Qt.Key_6); keyClick(Qt.Key_4); keyClick(Qt.Key_Tab);
                compare(memory.value, 64);
                mouseClick(findChild(settings, "applyTimeshiftSettings"));
                compare(backend.requests[0][1], 64);
            }
            function test_disabled_timeshift_keeps_budgets_but_disables_editing() {
                mouseClick(findChild(settings, "timeshiftEnabled"));
                compare(findChild(settings, "timeshiftMemoryLimit").enabled, false);
                mouseClick(findChild(settings, "applyTimeshiftSettings"));
                compare(backend.requests.length, 1);
                compare(backend.requests[0][0], "off");
            }
        }
    }
}
