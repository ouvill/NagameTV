import QtQuick
import MinimalViewer
import QtQuick.Controls
import QtQuick.Layouts

ColumnLayout {
    id: root
    required property real textSize
    required property real textOpacity
    required property real speed
    readonly property list<Item> navigationItems: [sizeSlider, opacitySlider, speedSlider]
    signal adjusted(real textSize, real textOpacity, real speed)
    spacing: 0
    RowLayout {
        Layout.fillWidth: true
        Label { text: qsTranslate("Main", "Text size"); color: Theme.textSecondary; font.pixelSize: Theme.fontBody }
        Item { Layout.fillWidth: true }
        Label { text: Math.round(root.textSize) + " px"; color: Theme.textPrimary; font.pixelSize: Theme.fontBody }
    }
    ThemedSlider {
        id: sizeSlider
        objectName: "danmakuTextSize"
        Accessible.name: qsTranslate("Main", "Text size")
        Layout.fillWidth: true
        leftPadding: 0; rightPadding: 0
        from: 14; to: 72; stepSize: 1
        value: root.textSize
        onMoved: root.adjusted(value, root.textOpacity, root.speed)
    }
    RowLayout {
        Layout.topMargin: 24
        Layout.fillWidth: true
        Label { text: qsTranslate("Settings", "Text opacity"); color: Theme.textSecondary; font.pixelSize: Theme.fontBody }
        Item { Layout.fillWidth: true }
        Label { text: Math.round((root.textOpacity) * 100) + "%"; color: Theme.textPrimary; font.pixelSize: Theme.fontBody }
    }
    ThemedSlider {
        id: opacitySlider
        objectName: "danmakuOpacity"
        Accessible.name: qsTranslate("Settings", "Text opacity")
        Layout.fillWidth: true
        leftPadding: 0; rightPadding: 0
        from: 0; to: 1; stepSize: 0.05
        value: root.textOpacity
        onMoved: root.adjusted(root.textSize, value, root.speed)
    }
    RowLayout {
        Layout.topMargin: 24
        Layout.fillWidth: true
        Label { text: qsTranslate("Main", "Comment speed"); color: Theme.textSecondary; font.pixelSize: Theme.fontBody }
        Item { Layout.fillWidth: true }
        Label { text: (root.speed).toFixed(1) + "×"; color: Theme.textPrimary; font.pixelSize: Theme.fontBody }
    }
    ThemedSlider {
        id: speedSlider
        objectName: "danmakuSpeed"
        Accessible.name: qsTranslate("Main", "Comment speed")
        Layout.fillWidth: true
        leftPadding: 0; rightPadding: 0
        from: 0.5; to: 2; stepSize: 0.1
        value: root.speed
        onMoved: root.adjusted(root.textSize, root.textOpacity, value)
    }
}
