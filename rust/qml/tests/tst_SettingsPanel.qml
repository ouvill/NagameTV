import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtTest
import ".."

TestCase {
    id: testCase
    name: "SettingsPanel"
    when: windowShown
    visible: true
    width: 900
    height: 560
    QtObject {
        id: backend
        property string server: "http://example.test:40772"
        property bool loading: false
        property bool subtitles_enabled: true
        property bool subtitle_display: true
        property bool epg_enabled: true
        property bool autoplay: false
        property bool remote_enabled: false
        property string remote_address: "0.0.0.0"
        property int remote_port: 50051
        property string remote_status: "disabled"
        property string remote_error: ""
        property string remote_save_error: ""
        property string remote_endpoints: ""
        property bool remote_session_only: false
        property bool acceptRemote: true
        signal remoteRequested(bool enabled, string address, int port)
        function configure_remote(enabled, address, port) {
            remoteRequested(enabled, address, port);
            if (!acceptRemote) return false;
            remote_enabled = enabled;
            remote_address = address;
            remote_port = port;
            return true;
        }
        function refresh_remote_addresses() {}
        property string screenshot_directory: "/pictures/mirakurun-viewer"
        property string screenshot_error: ""
        property string screenshot_format: "png"
        property string screenshot_options: JSON.stringify({png_compression: 6, jpg_quality: 90,
            webp_quality: 90, webp_mode: "lossy", ranges: {
                png: {min: 0, max: 9}, jpg: {min: 1, max: 100}, webp: {min: 1, max: 99}}})
        function configure_screenshot_format(value) { screenshot_format = value; return true; }
        function configure_screenshot_options(format, value, lossless) {
            const options = JSON.parse(screenshot_options);
            if (format === "png") options.png_compression = value;
            else if (format === "jpg") options.jpg_quality = value;
            else { options.webp_quality = value; options.webp_mode = lossless ? "lossless" : "lossy"; }
            screenshot_options = JSON.stringify(options);
            return true;
        }
        property int screenshotFolderRequests: 0
        function screenshot_directory_url() { return "file://" + screenshot_directory; }
        function configure_screenshot_directory(value) { screenshot_directory = value.toString().replace(/^file:\/\//, ""); return true; }
        function open_screenshot_directory() { screenshotFolderRequests++; return true; }
        function reset_screenshot_directory() { screenshot_directory = "/pictures/mirakurun-viewer"; return true; }
        function configure_autoplay(value) { autoplay = value; }
        property string settings_error: ""
        property string diagnostics: ""
        property string status: ""
        property string language: "en"
        property string subtitle_status: ""
        property bool comments_enabled: false
        property bool comments_allowed: true
        property bool danmaku_enabled: false
        property real comment_font_size: 21
        property real comment_opacity: 1
        property string comment_display: "scroll"
        property string comment_placement: "sequential"
        property bool evaluation_collision_layout: false
        function configure_comment_presentation(display, placement) { comment_display = display; comment_placement = placement; return true; }
        property real comment_speed: 1
        property bool comment_shadow_enabled: true
        function configure_comment_shadow(value) { comment_shadow_enabled = value; }
        property string comment_status: ""
        property bool comment_send_on_enter: false
        function configure_comment_send_on_enter(value) { comment_send_on_enter = value; }
        property string log_error: ""
        property bool acceptLanguage: true
        function request_language(value) { if (!acceptLanguage) return false; language = value; return true; }
        function display_subtitles(value) { subtitle_display = value; }
        function enable_comments(value) { comments_enabled = value; }
        function configure_danmaku(value, size, opacity, speed) {
            danmaku_enabled = value;
            comment_font_size = size;
            comment_opacity = opacity;
            comment_speed = speed;
            return true;
        }
        property int logFolderRequests: 0
        function open_log_folder() { logFolderRequests++; return true; }
        signal connectRequested(string url)
        signal connectionFinished(bool success, int channels)
        property bool acceptConnection: false
        function connect_server(url) { connectRequested(url); return acceptConnection; }
    }
    SettingsPanel {
        id: panel
        backend: backend
        iconDirectory: Qt.resolvedUrl("../../../assets/icons/")
        onStatsRequested: function(value) { statsVisible = value; }
    }
    SignalSpy { id: connections; target: backend; signalName: "connectRequested" }
    SignalSpy { id: accepted; target: panel; signalName: "connectionAccepted" }
    SignalSpy { id: remoteRequests; target: backend; signalName: "remoteRequested" }
    function init() {
        failOnWarning(/.*/);
        backend.loading = false;
        backend.acceptConnection = false;
        backend.diagnostics = "";
        backend.status = "";
        backend.settings_error = "";
        backend.server = "http://example.test:40772";
        backend.language = "en";
        backend.acceptLanguage = true;
        backend.subtitles_enabled = true;
        backend.subtitle_display = true;
        backend.epg_enabled = true;
        backend.autoplay = false;
        backend.remote_enabled = false;
        backend.remote_address = "0.0.0.0";
        backend.remote_port = 50051;
        backend.remote_status = "disabled";
        backend.remote_error = "";
        backend.remote_save_error = "";
        backend.remote_endpoints = "";
        backend.remote_session_only = false;
        backend.acceptRemote = true;
        remoteRequests.clear();
        backend.screenshot_directory = "/pictures/mirakurun-viewer";
        backend.screenshot_error = "";
        backend.screenshot_format = "png";
        backend.configure_screenshot_options("png", 6, false);
        backend.configure_screenshot_options("jpg", 90, false);
        backend.configure_screenshot_options("webp", 90, false);
        backend.screenshotFolderRequests = 0;
        backend.comments_enabled = false;
        backend.comments_allowed = true;
        backend.danmaku_enabled = false;
        backend.comment_font_size = 21;
        backend.comment_opacity = 1;
        backend.comment_speed = 1;
        backend.comment_shadow_enabled = true;
        backend.comment_send_on_enter = false;
        backend.log_error = "";
        panel.statsVisible = false;
        panel.page = SettingsPanel.Connection;
        connections.clear();
        accepted.clear();
        panel.open();
        tryCompare(panel, "opened", true);
    }
    function cleanup() { panel.close(); tryCompare(panel, "visible", false); }
    function test_remote_defaults_can_be_enabled_and_binding_failures_remain_visible() {
        selectPage(SettingsPanel.Remote);
        const toggle = findChild(panel.contentItem, "remoteEnabled");
        compare(findChild(panel.contentItem, "remoteAddress").text, "0.0.0.0");
        compare(findChild(panel.contentItem, "remotePort").text, "50051");
        mouseClick(toggle);
        compare(remoteRequests.count, 1);
        compare(remoteRequests.signalArguments[0][0], true);
        compare(remoteRequests.signalArguments[0][1], "0.0.0.0");
        compare(remoteRequests.signalArguments[0][2], 50051);
        backend.remote_status = "failed";
        backend.remote_error = "Port is already in use";
        compare(toggle.checked, true);
        compare(findChild(panel.contentItem, "remoteError").text, backend.remote_error);
        compare(findChild(panel.contentItem, "remoteError").visible, true);
        findChild(panel.contentItem, "remoteAddress").text = "invalid draft";
        findChild(panel.contentItem, "remotePort").text = "0";
        mouseClick(toggle);
        compare(backend.remote_enabled, false);
        compare(backend.remote_port, 50051);
    }
    function test_remote_invalid_edits_do_not_enable_and_reopening_discards_draft() {
        selectPage(SettingsPanel.Remote);
        backend.acceptRemote = false;
        findChild(panel.contentItem, "remoteAddress").text = "invalid";
        mouseClick(findChild(panel.contentItem, "remoteEnabled"));
        compare(backend.remote_enabled, false);
        compare(findChild(panel.contentItem, "remoteEnabled").checked, false);
        compare(findChild(panel.contentItem, "remoteInvalidInput").visible, true);
        panel.close();
        tryCompare(panel, "visible", false);
        panel.open();
        tryCompare(panel, "opened", true);
        compare(findChild(panel.contentItem, "remoteAddress").text, "0.0.0.0");
    }
    function test_remote_port_edits_and_connection_addresses() {
        selectPage(SettingsPanel.Remote);
        findChild(panel.contentItem, "remotePort").text = "50052";
        const button = findChild(panel.contentItem, "applyRemote");
        button.forceActiveFocus();
        keyClick(Qt.Key_Space);
        compare(backend.remote_port, 50052);
        compare(backend.remote_enabled, false);
        backend.remote_session_only = true;
        backend.remote_status = "listening";
        backend.remote_endpoints = "192.168.1.10:50052";
        compare(findChild(panel.contentItem, "remoteSessionOnly").visible, true);
        compare(findChild(panel.contentItem, "remoteEndpoints").text, "192.168.1.10:50052");
    }
    function selectPage(index) {
        mouseClick(findChild(panel.contentItem, "settingsCategory" + index));
        compare(panel.page, index);
    }
    function test_comment_send_shortcut_tracks_setting_and_can_be_toggled() {
        selectPage(SettingsPanel.Comments);
        const flick = findChild(panel.contentItem, "settingsFlickable");
        flick.contentY = flick.contentHeight - flick.height;
        const choice = findChild(panel.contentItem, "commentSendKey");
        backend.comment_send_on_enter = false;
        compare(choice.currentIndex, 0);
        choice.forceActiveFocus();
        keyClick(Qt.Key_Down);
        compare(backend.comment_send_on_enter, true);
        compare(choice.currentIndex, 1);
        keyClick(Qt.Key_Up);
        compare(backend.comment_send_on_enter, false);
        backend.comment_send_on_enter = true;
        compare(choice.currentIndex, 1);
        keyClick(Qt.Key_Space);
        tryCompare(choice.popup, "opened", true);
        const popupPosition = choice.popup.contentItem.mapToItem(panel.contentItem, 0, 0);
        verify(popupPosition.y >= 0);
        verify(popupPosition.y + choice.popup.contentItem.height <= panel.height);
        keyClick(Qt.Key_Escape);
        tryCompare(choice.popup, "visible", false);
        compare(panel.opened, true);
    }
    function test_autoplay_tracks_preference_without_connecting() {
        const flick = findChild(panel.contentItem, "settingsFlickable");
        flick.contentY = Math.max(0, flick.contentHeight - flick.height);
        const toggle = findChild(panel.contentItem, "autoplaySetting");
        compare(toggle.checked, false);
        mouseClick(toggle);
        compare(backend.autoplay, true);
        compare(connections.count, 0);
        backend.autoplay = false;
        compare(toggle.checked, false);
        toggle.forceActiveFocus();
        keyClick(Qt.Key_Space);
        compare(backend.autoplay, true);
        panel.close();
        tryCompare(panel, "visible", false);
        panel.open();
        tryCompare(panel, "opened", true);
        compare(toggle.checked, true);
        compare(connections.count, 0);
        flick.contentY = 0;
    }
    function test_connection_uses_edited_url_without_duplicate_loading_request() {
        const field = findChild(panel.contentItem, "serverField");
        const connect = findChild(panel.contentItem, "connectServer");
        field.text = "http://new.example:40772";
        mouseClick(connect);
        compare(connections.count, 1);
        compare(connections.signalArguments[0][0], field.text);
        backend.loading = true;
        field.forceActiveFocus();
        keyClick(Qt.Key_Return);
        compare(connections.count, 1);
        compare(connect.enabled, false);
    }
    function test_connection_waits_for_response_and_success_requires_continue() {
        panel.connectToServer();
        compare(panel.opened, true);
        compare(accepted.count, 0);
        compare(findChild(panel.contentItem, "connectionResult").visible, true);
        backend.acceptConnection = true;
        panel.connectToServer();
        compare(panel.opened, true);
        compare(accepted.count, 0);
        backend.connectionFinished(false, 0);
        compare(panel.opened, true);
        compare(accepted.count, 0);
        panel.connectToServer();
        backend.connectionFinished(true, 4);
        compare(panel.opened, true);
        compare(accepted.count, 0);
        const button = findChild(panel.contentItem, "connectServer");
        button.forceActiveFocus();
        keyClick(Qt.Key_Space);
        tryCompare(panel, "visible", false);
        compare(accepted.count, 1);
    }
    function test_closed_panel_ignores_late_connection_result() {
        backend.acceptConnection = true;
        panel.connectToServer();
        panel.close();
        tryCompare(panel, "visible", false);
        backend.connectionFinished(true, 4);
        compare(accepted.count, 0);
        panel.open();
        tryCompare(panel, "opened", true);
        compare(findChild(panel.contentItem, "connectionResult").visible, false);
    }
    function test_full_window_layout_and_escape_close() {
        compare(panel.width, panel.parent.width);
        compare(panel.height, panel.parent.height);
        compare(panel.x, 0);
        compare(panel.y, 0);
        const nav = findChild(panel.contentItem, "settingsNavigation");
        const scroll = findChild(panel.contentItem, "settingsScroll");
        const footer = findChild(panel.contentItem, "settingsFooter");
        verify(nav.x + nav.width < scroll.x);
        verify(scroll.y + scroll.height < footer.y);
        verify(footer.y + footer.height < panel.height);
        keyClick(Qt.Key_Escape);
        tryCompare(panel, "visible", false);
    }
    function test_server_draft_survives_category_switch_but_reopening_uses_saved_url() {
        const field = findChild(panel.contentItem, "serverField");
        field.text = "http://draft.example:40772";
        selectPage(SettingsPanel.Comments);
        selectPage(SettingsPanel.Connection);
        compare(field.text, "http://draft.example:40772");
        compare(connections.count, 0);
        mouseClick(findChild(panel.contentItem, "closeSettings"));
        tryCompare(panel, "visible", false);
        panel.open();
        tryCompare(panel, "opened", true);
        compare(field.text, backend.server);
    }
    function test_subtitle_visibility_is_a_viewer_setting_independent_of_processing() {
        selectPage(SettingsPanel.Display);
        const subtitles = findChild(panel.contentItem, "subtitleDisplay");
        compare(subtitles.visible, true);
        compare(subtitles.checked, true);
        mouseClick(subtitles);
        compare(backend.subtitle_display, false);
        compare(backend.subtitles_enabled, true);
        compare(backend.epg_enabled, true);
        // The playback bar calls the same backend action; reflect its change here.
        backend.display_subtitles(true);
        compare(subtitles.checked, true);
        subtitles.forceActiveFocus();
        keyClick(Qt.Key_Space);
        compare(backend.subtitle_display, false);
        compare(backend.subtitles_enabled, true);
        compare(connections.count, 0);
        selectPage(SettingsPanel.Diagnostics);
        compare(subtitles.visible, false);
    }
    function test_screenshot_formats_are_exclusive_and_keyboard_accessible() {
        selectPage(SettingsPanel.Display);
        const png = findChild(panel.contentItem, "screenshotFormat_png");
        const jpg = findChild(panel.contentItem, "screenshotFormat_jpg");
        const webp = findChild(panel.contentItem, "screenshotFormat_webp");
        verify(png.checked); verify(!jpg.checked); verify(!webp.checked);
        for (const key of ["jpg", "webp", "png"]) {
            const option = findChild(panel.contentItem, "screenshotFormat_" + key);
            option.forceActiveFocus();
            keyClick(Qt.Key_Space);
            compare(backend.screenshot_format, key);
            compare(png.checked, key === "png");
            compare(jpg.checked, key === "jpg");
            compare(webp.checked, key === "webp");
            verify(findChild(panel.contentItem, "screenshotFormatDescription").text.length > 0);
            verify(option.contentItem.height > 0);
        }
        backend.screenshot_format = "webp";
        verify(webp.checked); verify(!png.checked);
    }
    function test_screenshot_parameters_preserve_each_format_and_lossless_hides_quality() {
        selectPage(SettingsPanel.Display);
        const number = findChild(panel.contentItem, "screenshotParameterNumber");
        const slider = findChild(panel.contentItem, "screenshotParameterSlider");
        const lossless = findChild(panel.contentItem, "screenshotWebpLossless");
        compare(number.value, 6); compare(number.from, 0); compare(number.to, 9);
        number.forceActiveFocus(); keyClick(Qt.Key_Up);
        compare(JSON.parse(backend.screenshot_options).png_compression, 7);
        compare(slider.value, 7);
        backend.configure_screenshot_format("jpg");
        compare(number.value, 90); compare(number.to, 100);
        number.forceActiveFocus(); keyClick(Qt.Key_Down);
        backend.configure_screenshot_format("webp");
        compare(number.value, 90); compare(number.to, 99);
        lossless.forceActiveFocus(); keyClick(Qt.Key_Space);
        compare(JSON.parse(backend.screenshot_options).webp_mode, "lossless");
        verify(!number.visible); verify(!slider.visible);
        keyClick(Qt.Key_Space);
        verify(number.visible); compare(number.value, 90);
        backend.configure_screenshot_format("jpg"); compare(number.value, 89);
        backend.configure_screenshot_format("png"); compare(number.value, 7);
    }
    function test_screenshot_folder_controls_follow_setting_and_show_errors() {
        selectPage(SettingsPanel.Display);
        const flick = findChild(panel.contentItem, "settingsFlickable");
        flick.contentY = Math.max(0, flick.contentHeight - flick.height);
        const path = findChild(panel.contentItem, "screenshotDirectoryPath");
        compare(path.text, backend.screenshot_directory);
        const open = findChild(panel.contentItem, "openScreenshotDirectory");
        open.forceActiveFocus();
        keyClick(Qt.Key_Space);
        compare(backend.screenshotFolderRequests, 1);
        backend.configure_screenshot_directory("file:///custom/画像");
        compare(path.text, "/custom/画像");
        const reset = findChild(panel.contentItem, "resetScreenshotDirectory");
        reset.forceActiveFocus();
        keyClick(Qt.Key_Space);
        compare(path.text, "/pictures/mirakurun-viewer");
        const error = findChild(panel.contentItem, "screenshotFolderError");
        compare(error.visible, false);
        backend.screenshot_error = "Cannot write this folder";
        compare(error.visible, true);
        compare(error.text, backend.screenshot_error);
    }
    function test_screenshot_folder_picker_applies_only_after_acceptance() {
        backend.configure_screenshot_directory(Qt.resolvedUrl("."));
        const previous = backend.screenshot_directory;
        selectPage(SettingsPanel.Display);
        const dialog = findChild(panel, "screenshotFolderDialog");
        dialog.options = FolderDialog.DontUseNativeDialog;
        const choose = findChild(panel.contentItem, "chooseScreenshotDirectory");
        choose.forceActiveFocus();
        keyClick(Qt.Key_Space);
        tryCompare(dialog, "visible", true);
        dialog.selectedFolder = Qt.resolvedUrl("..");
        dialog.reject();
        tryCompare(dialog, "visible", false);
        compare(backend.screenshot_directory, previous);
        // Dialog dismissal restores focus asynchronously; wait for the parent to render.
        verify(waitForRendering(panel.contentItem));
        choose.forceActiveFocus();
        tryCompare(choose, "activeFocus", true);
        keyClick(Qt.Key_Space);
        tryCompare(dialog, "visible", true);
        dialog.selectedFolder = Qt.resolvedUrl("..");
        const selected = dialog.selectedFolder.toString().replace(/^file:\/\//, "");
        dialog.accept();
        tryCompare(dialog, "visible", false);
        compare(backend.screenshot_directory, selected);
    }
    function test_development_feature_limit_does_not_enable_processing_from_display() {
        backend.subtitles_enabled = false;
        backend.subtitle_display = true;
        selectPage(SettingsPanel.Display);
        const subtitles = findChild(panel.contentItem, "subtitleDisplay");
        compare(subtitles.enabled, false);
        compare(subtitles.checked, false);
        mouseClick(subtitles);
        compare(backend.subtitles_enabled, false);
        compare(backend.subtitle_display, true);
    }
    function test_comment_rows_and_sliders_apply_to_the_backend() {
        selectPage(SettingsPanel.Comments);
        mouseClick(findChild(panel.contentItem, "commentsEnabled"));
        compare(backend.comments_enabled, true);
        mouseClick(findChild(panel.contentItem, "danmakuEnabled"));
        compare(backend.danmaku_enabled, true);
        const size = findChild(findChild(panel.contentItem, "commentSize"), "settingSlider");
        size.forceActiveFocus();
        keyClick(Qt.Key_Right);
        compare(backend.comment_font_size, 22);
        for (let value = 22; value < 72; ++value)
            keyClick(Qt.Key_Right);
        compare(backend.comment_font_size, 72);
        keyClick(Qt.Key_Right);
        compare(backend.comment_font_size, 72);
        const opacity = findChild(findChild(panel.contentItem, "commentOpacity"), "settingSlider");
        opacity.forceActiveFocus();
        keyClick(Qt.Key_Left);
        fuzzyCompare(backend.comment_opacity, 0.95, 0.001);
        const speed = findChild(findChild(panel.contentItem, "commentSpeed"), "settingSlider");
        speed.forceActiveFocus();
        keyClick(Qt.Key_Right);
        fuzzyCompare(backend.comment_speed, 1.1, 0.001);
    }
    function test_comment_shadow_toggle_tracks_saved_value_and_disabled_reception() {
        selectPage(SettingsPanel.Comments);
        const shadow = findChild(panel.contentItem, "commentShadow");
        compare(shadow.enabled, false);
        compare(shadow.checked, true);
        backend.comments_enabled = true;
        shadow.forceActiveFocus();
        keyClick(Qt.Key_Space);
        compare(backend.comment_shadow_enabled, false);
        backend.comment_shadow_enabled = true;
        compare(shadow.checked, true);
        backend.comments_enabled = false;
        keyClick(Qt.Key_Space);
        compare(backend.comment_shadow_enabled, true);
    }
    function test_language_rejection_restores_selection_and_popup_owns_escape() {
        selectPage(SettingsPanel.Display);
        const language = findChild(panel.contentItem, "languageSetting");
        backend.acceptLanguage = false;
        language.forceActiveFocus();
        keyClick(Qt.Key_Space);
        tryCompare(language.popup, "opened", true);
        keyClick(Qt.Key_Up);
        keyClick(Qt.Key_Return);
        tryCompare(language.popup, "visible", false);
        compare(backend.language, "en");
        compare(language.currentIndex, 2);
        compare(findChild(panel.contentItem, "languageError").visible, true);
        language.forceActiveFocus();
        keyClick(Qt.Key_Space);
        tryCompare(language.popup, "opened", true);
        keyClick(Qt.Key_Escape);
        tryCompare(language.popup, "visible", false);
        compare(panel.opened, true);
        panel.close();
        tryCompare(panel, "visible", false);
        panel.open();
        tryCompare(panel, "opened", true);
        compare(findChild(panel.contentItem, "languageError").visible, false);
    }
    function test_error_details_are_available_without_overwhelming_the_message() {
        backend.settings_error = "Cannot read <settings>: " + "technical details ".repeat(80);
        const problem = findChild(panel.contentItem, "settingsError");
        const details = findChild(problem, "problemDetails");
        const disclosure = findChild(problem, "problemDetailsToggle");
        compare(problem.visible, true);
        compare(details.visible, false);
        mouseClick(disclosure);
        compare(details.visible, true);
        compare(details.text, backend.settings_error);
        compare(details.textFormat, Text.PlainText);
        compare(details.truncated, false);
        backend.settings_error = "A different error";
        compare(details.visible, false);
        backend.settings_error = "";
        compare(problem.visible, false);
    }
    function test_scroll_keeps_navigation_and_return_visible_and_resets_on_category_change() {
        backend.diagnostics = "Diagnostic details ".repeat(200);
        selectPage(SettingsPanel.Diagnostics);
        const flick = findChild(panel.contentItem, "settingsFlickable");
        const scroll = findChild(panel.contentItem, "settingsScroll");
        const nav = findChild(panel.contentItem, "settingsNavigation");
        const footer = findChild(panel.contentItem, "settingsFooter");
        const navY = nav.y;
        const footerY = footer.y;
        tryVerify(() => flick.contentHeight > flick.height);
        compare(scroll.ScrollBar.vertical.policy, ScrollBar.AlwaysOn);
        flick.contentY = flick.contentHeight - flick.height;
        compare(nav.y, navY);
        compare(footer.y, footerY);
        selectPage(SettingsPanel.Connection);
        tryCompare(flick, "contentY", 0);
        compare(scroll.ScrollBar.vertical.policy, flick.contentHeight > flick.height ? ScrollBar.AlwaysOn : ScrollBar.AlwaysOff);
        compare(findChild(panel.contentItem, "serverField").visible, true);
    }
    function test_diagnostics_remain_readable_data() {
        return [
            {tag: "English", text: "Subtitles: subscriptions 8, pending 128, received 18446744073709551615 | EPG: tasks 1, programs 50000, stopping Yes"},
            {tag: "Japanese", text: "字幕: 購読 8, 待機 128, 受信 18446744073709551615 | EPG: タスク 1, 番組 50000, 停止待ち はい"}
        ];
    }
    function test_diagnostics_remain_readable(data) {
        backend.diagnostics = data.text;
        selectPage(SettingsPanel.Diagnostics);
        const label = findChild(panel.contentItem, "featureDiagnostics");
        verify(label !== null);
        tryVerify(() => label.lineCount > 1);
        compare(label.truncated, false);
        compare(label.text, data.text);
        verify(label.width <= findChild(panel.contentItem, "settingsScroll").width);
        tryVerify(() => label.height >= label.contentHeight);
    }
}
