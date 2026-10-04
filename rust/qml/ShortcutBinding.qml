import QtQuick
import QtQuick.Controls

Shortcut {
    required property Action operation
    required property InputContext inputContext
    required property int scope
    required property bool active
    property string description: operation.text
    property string condition: ""
    property bool available: true
    context: Qt.WindowShortcut
    autoRepeat: false
    enabled: active && operation.enabled && available && inputContext.accepts(scope)
    onActivated: operation.trigger()
    onActivatedAmbiguously: console.warn("Ambiguous shortcut: " + portableText)
}
