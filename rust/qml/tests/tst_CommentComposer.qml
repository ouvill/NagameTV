import QtQuick
import QtQuick.Controls
import QtTest
import MinimalViewer 1.0
import ".."

TestCase {
    id: testCase
    name: "CommentComposer"
    when: windowShown
    visible: true
    width: 420
    height: 320
    CommentSubmitPolicy { id: submitPolicy }
    CommentComposer {
        id: composer
        submitPolicy: submitPolicy
        width: 340
        available: true
        iconDirectory: Qt.resolvedUrl("../../../assets/icons/")
        onDraftEdited: function(text) { draft = text; }
    }
    TestInputMethod { id: ime }
    SignalSpy { id: sent; target: composer; signalName: "sendRequested" }
    function editor() { return findChild(composer, "commentEditor"); }
    function replaceText(text) {
        editor().remove(0, editor().length);
        editor().insert(0, text);
    }
    function init() {
        failOnWarning(/.*/);
        composer.visible = true;
        composer.busy = false;
        composer.available = true;
        submitPolicy.mode = CommentSubmitPolicy.ControlEnter;
        composer.status = "";
        composer.width = 340;
        composer.draft = "実況🦀";
        composer.focusEditor();
        tryCompare(editor(), "activeFocus", true);
        sent.clear();
    }
    function cleanup() { ime.compose("", ""); }
    function test_default_ctrl_enter_and_keypad_enter_send_but_enter_does_nothing() {
        editor().cursorPosition = editor().length;
        keyClick(Qt.Key_Return);
        compare(sent.count, 0);
        compare(composer.draft, "実況🦀");
        keyClick(Qt.Key_Return, Qt.ControlModifier);
        compare(sent.count, 1);
        keyClick(Qt.Key_Enter, Qt.ControlModifier | Qt.KeypadModifier);
        compare(sent.count, 2);
        keyClick(Qt.Key_Return, Qt.ShiftModifier);
        compare(sent.count, 2);
        compare(composer.draft, "実況🦀");
    }
    function test_enter_setting_shift_does_nothing_and_button_keeps_focus() {
        submitPolicy.mode = CommentSubmitPolicy.EnterOrControlEnter;
        keyClick(Qt.Key_Return);
        compare(sent.count, 1);
        keyClick(Qt.Key_Return, Qt.ShiftModifier);
        compare(sent.count, 1);
        compare(composer.draft, "実況🦀");
        keyClick(Qt.Key_Return, Qt.ControlModifier);
        compare(sent.count, 2);
        mouseClick(findChild(composer, "sendComment"));
        compare(sent.count, 3);
        verify(editor().activeFocus);
    }
    function test_busy_empty_and_unavailable_never_send_or_erase_draft() {
        composer.busy = true;
        keyClick(Qt.Key_Return, Qt.ControlModifier);
        keyClick(Qt.Key_X);
        compare(composer.draft, "実況🦀");
        compare(sent.count, 0);
        compare(editor().readOnly, true);
        composer.busy = false;
        composer.available = false;
        keyClick(Qt.Key_Return, Qt.ControlModifier);
        compare(sent.count, 0);
        composer.available = true;
        composer.draft = "　 \n\t";
        keyClick(Qt.Key_Return, Qt.ControlModifier);
        compare(sent.count, 0);
        compare(findChild(composer, "sendComment").enabled, false);
    }
    function test_ime_preedit_enter_and_ctrl_enter_are_not_posts() {
        submitPolicy.mode = CommentSubmitPolicy.EnterOrControlEnter;
        verify(ime.compose("じっきょう", ""));
        tryCompare(editor(), "inputMethodComposing", true);
        compare(findChild(composer, "sendComment").enabled, false);
        keyClick(Qt.Key_Return);
        compare(sent.count, 0);
        verify(ime.compose("じっきょう", ""));
        keyClick(Qt.Key_Return, Qt.ControlModifier);
        compare(sent.count, 0);
        verify(ime.compose("", "実況"));
        tryCompare(editor(), "inputMethodComposing", false);
        keyClick(Qt.Key_Return);
        compare(sent.count, 1);
    }
    function test_result_and_visibility_keep_text_until_backend_acknowledges() {
        composer.status = "Could not post <comment>";
        compare(editor().text, "実況🦀");
        composer.visible = false;
        composer.visible = true;
        compare(editor().text, "実況🦀");
        compare(findChild(composer, "commentPostHint").textFormat, Text.PlainText);
        composer.draft = "";
        compare(editor().text, "");
        replaceText("x".repeat(1023) + "🦀");
        compare(editor().length, 1023);
        compare(composer.draft.length, 1023);
    }
    function test_pasted_line_breaks_become_spaces_and_counter_tracks_edits() {
        replaceText("一行目\r\n二行目\n三行目\u2028終わり");
        compare(composer.draft, "一行目 二行目 三行目 終わり");
        compare(findChild(composer, "commentCharacterCount").text, composer.draft.length + " / 1024");
        compare(sent.count, 0);
    }
    function test_compact_row_keeps_editor_counter_and_button_separate() {
        for (const width of [340, 492, 760]) {
            composer.width = width;
            wait(0);
            compare(composer.height, 46);
            const counter = findChild(composer, "commentCharacterCount");
            const button = findChild(composer, "sendComment");
            verify(editor().width > 100);
            verify(editor().x + editor().width < counter.x);
            verify(counter.x + counter.width < button.x);
            verify(button.x + button.width <= composer.width);
        }
    }
    function test_feedback_reserves_space_and_clears_on_edit() {
        compare(composer.occupiedHeight, 46);
        composer.status = "Delivery could not be confirmed. Check the comments before sending again.";
        const hint = findChild(composer, "commentPostHint");
        verify(hint.visible);
        verify(composer.occupiedHeight > 46);
        composer.status = "";
        compare(hint.visible, false);
        compare(composer.occupiedHeight, 46);
        composer.status = "Another result";
        verify(hint.visible);
        replaceText("次のコメント");
        compare(hint.visible, false);
        compare(composer.occupiedHeight, 46);
    }
}
