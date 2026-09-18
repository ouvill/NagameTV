import QtQuick

QtObject {
    id: root
    enum Mode { ControlEnter, EnterOrControlEnter }
    enum Disposition { PassThrough, Consume, Submit }
    property int mode: CommentSubmitPolicy.ControlEnter
    readonly property string keys: mode === CommentSubmitPolicy.EnterOrControlEnter ? "Enter / Ctrl + Enter" : "Ctrl + Enter"
    readonly property string hint: mode === CommentSubmitPolicy.EnterOrControlEnter
        ? qsTranslate("Viewer", "Enter to send") : qsTranslate("Viewer", "Ctrl+Enter to send")

    function disposition(event, composing) {
        if (composing || (event.key !== Qt.Key_Return && event.key !== Qt.Key_Enter))
            return CommentSubmitPolicy.PassThrough;
        const modifiers = event.modifiers & ~Qt.KeypadModifier;
        const submit = modifiers === Qt.ControlModifier
            || (mode === CommentSubmitPolicy.EnterOrControlEnter && modifiers === Qt.NoModifier);
        return submit && !event.isAutoRepeat ? CommentSubmitPolicy.Submit : CommentSubmitPolicy.Consume;
    }
}
