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
                backend.requests = [];
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
