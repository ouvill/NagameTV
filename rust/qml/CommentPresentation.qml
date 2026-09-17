import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ColumnLayout {
    id: root
    property string displayMode: "scroll"
    property string placementMode: "sequential"
    property bool evaluationCollision: false
    signal selected(string displayMode, string placementMode)
    spacing: 6
    Label { text: qsTranslate("Main", "Comment motion"); color: "#b6bab6"; font.pixelSize: 12 }
    SettingsChoice {
        objectName: "commentMotion"
        Layout.fillWidth: true
        implicitHeight: 40
        model: [qsTranslate("Main", "Scroll"), qsTranslate("Main", "Fountain")]
        currentIndex: root.displayMode === "pop" ? 1 : 0
        onActivated: function(index) {
            root.selected(index === 1 ? "pop" : "scroll", index === 1 && root.placementMode === "collision" ? "sequential" : root.placementMode);
        }
    }
    Label { text: qsTranslate("Main", "Comment placement"); color: "#b6bab6"; font.pixelSize: 12 }
    SettingsChoice {
        objectName: "commentPlacement"
        Layout.fillWidth: true
        implicitHeight: 40
        readonly property bool legacyAvailable: root.evaluationCollision && root.displayMode === "scroll"
        model: legacyAvailable
            ? [qsTranslate("Main", "Even spread"), qsTranslate("Main", "Random"), qsTranslate("Main", "Legacy collision layout (evaluation)")]
            : [qsTranslate("Main", "Even spread"), qsTranslate("Main", "Random")]
        currentIndex: root.placementMode === "collision" && legacyAvailable ? 2 : root.placementMode === "random" ? 1 : 0
        onActivated: function(index) { root.selected(root.displayMode, ["sequential", "random", "collision"][index]); }
    }
}
