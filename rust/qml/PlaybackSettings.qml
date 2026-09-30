pragma ComponentBehavior: Bound
import QtQuick
import MinimalViewer
import QtQuick.Controls
import QtQuick.Layouts

ScrollView {
    id: root
    required property bool commentsEnabled
    required property bool danmakuEnabled
    required property real textSize
    required property real textOpacity
    required property real speed
    required property bool shadowEnabled
    required property bool statsVisible
    property string displayMode: "scroll"
    property string placementMode: "sequential"
    property string densityMode: "normal"
    property bool evaluationCollision: false
    property bool appearanceExpanded: false
    signal densityRequested(string densityMode)
    signal presentationRequested(string displayMode, string placementMode)
    signal danmakuRequested(bool enabled)
    signal adjusted(real textSize, real textOpacity, real speed)
    signal statsRequested(bool visible)
    signal shadowRequested(bool enabled)
    signal boundaryReached(int key)
    function enter() { navigation.enter(); }
    function enterLast() { navigation.enterLast(); }
    clip: true
    contentWidth: availableWidth
    FormNavigation {
        id: navigation
        scrollView: root
        fields: [danmakuToggle, ...adjustments.navigationItems, appearanceButton,
            ...presentation.navigationItems, shadowToggle, statsToggle]
        onBoundaryReached: function(key) { root.boundaryReached(key); }
    }

    ColumnLayout {
        width: root.availableWidth
        spacing: Theme.spaceLg
        SettingsToggle {
            id: danmakuToggle
            objectName: "playbackDanmakuToggle"
            Layout.fillWidth: true
            enabled: root.commentsEnabled
            text: qsTranslate("Main", "Danmaku comments")
            checked: root.danmakuEnabled
            onToggled: root.danmakuRequested(checked)
        }
        DanmakuAdjustments {
            id: adjustments
            Layout.fillWidth: true
            enabled: root.commentsEnabled
            textSize: root.textSize
            textOpacity: root.textOpacity
            speed: root.speed
            onAdjusted: function(size, opacity, speed) { root.adjusted(size, opacity, speed); }
        }
        DisclosureButton {
            id: appearanceButton
            objectName: "playbackAppearanceButton"
            Layout.fillWidth: true
            enabled: root.commentsEnabled
            checked: root.appearanceExpanded
            text: qsTranslate("Settings", "Comment appearance")
            onToggled: root.appearanceExpanded = checked
        }
        CommentPresentation {
            id: presentation
            Layout.fillWidth: true
            visible: root.appearanceExpanded
            enabled: root.commentsEnabled
            displayMode: root.displayMode
            placementMode: root.placementMode
            densityMode: root.densityMode
            evaluationCollision: root.evaluationCollision
            onDensitySelected: function(density) { root.densityRequested(density); }
            onSelected: function(display, placement) { root.presentationRequested(display, placement); }
        }
        SettingsToggle {
            id: shadowToggle
            objectName: "playbackShadowToggle"
            Layout.fillWidth: true
            enabled: root.commentsEnabled
            visible: root.appearanceExpanded
            text: qsTranslate("Settings", "Drop shadow")
            checked: root.shadowEnabled
            onToggled: root.shadowRequested(checked)
        }
        Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.divider }
        SettingsToggle {
            id: statsToggle
            objectName: "playbackStatsToggle"
            Layout.fillWidth: true
            text: qsTranslate("Main", "Stats for nerds")
            checked: root.statsVisible
            onToggled: root.statsRequested(checked)
        }
    }
}
