pragma ComponentBehavior: Bound
import QtQuick
import MinimalViewer
import QtQuick.Controls
import QtQuick.Layouts

ColumnLayout {
    id: root
    property string displayMode: "scroll"
    property string placementMode: "sequential"
    property bool evaluationCollision: false
    signal selected(string displayMode, string placementMode)
    spacing: Theme.spaceSm
    Label { text: qsTranslate("Main", "Comment motion"); color: Theme.textSecondary; font.pixelSize: Theme.fontCaption }
    SegmentedControl {
        objectName: "commentMotion"
        objectNamePrefix: "motion-"
        Layout.fillWidth: true
        options: [{value: "scroll", label: qsTranslate("Main", "Scroll")},
            {value: "pop", label: qsTranslate("Main", "Fountain")}]
        value: root.displayMode
        onSelected: function(value) {
            root.selected(value, value === "pop" && root.placementMode === "collision" ? "sequential" : root.placementMode);
        }
    }
    Label {
        Layout.topMargin: 8
        text: qsTranslate("Main", "Comment placement"); color: Theme.textSecondary; font.pixelSize: Theme.fontCaption
    }
    SegmentedControl {
        objectName: "commentPlacement"
        objectNamePrefix: "placement-"
        Layout.fillWidth: true
        readonly property bool legacyAvailable: root.evaluationCollision && root.displayMode === "scroll"
        options: [{value: "sequential", label: qsTranslate("Main", "Even spread")},
            {value: "random", label: qsTranslate("Main", "Random")}].concat(legacyAvailable
                ? [{value: "collision", label: qsTranslate("Main", "Legacy collision layout (evaluation)")}] : [])
        value: root.placementMode
        onSelected: function(value) { root.selected(root.displayMode, value); }
    }
}
