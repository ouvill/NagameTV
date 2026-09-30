pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import MinimalViewer

// Shared presentation only; the timeline owns the confirmed and pending positions.
ThemedSlider {
    id: slider
    required property string positionText
    property real indicatedValue: value
    property real thumbSize: Theme.seekThumbSize
    property bool thumbVisible: true
    enum Emphasis { Quiet, Pointer, Feedback, Focus }
    readonly property int emphasis: !enabled ? SeekSlider.Quiet
        : visualFocus ? SeekSlider.Focus : feedback.running ? SeekSlider.Feedback
        : pressed || hovered ? SeekSlider.Pointer : SeekSlider.Quiet
    readonly property bool emphasized: emphasis !== SeekSlider.Quiet
    readonly property bool feedbackActive: feedback.running
    readonly property bool positionLabelVisible: enabled && !pressed
        && (feedbackActive || (visualFocus && !hovered))
    readonly property real trackThickness: emphasized ? Theme.seekTrackActiveHeight : Theme.seekTrackHeight
    readonly property real progressThickness: emphasized ? Theme.seekProgressActiveHeight : Theme.seekProgressHeight
    readonly property real retainedThickness: emphasized ? Theme.seekRetainedActiveHeight : Theme.seekRetainedHeight

    function flashSeek() {
        if (enabled && visible) feedback.restart();
    }
    onEnabledChanged: if (!enabled) feedback.stop()
    onVisibleChanged: if (!visible) feedback.stop()
    Timer { id: feedback; interval: Theme.seekFeedbackDuration }

    background: Rectangle {
        x: slider.leftPadding + slider.thumbSize / 2
        y: slider.topPadding + slider.availableHeight / 2 - height / 2
        width: slider.availableWidth - slider.thumbSize
        height: slider.trackThickness
        radius: height / 2
        color: Theme.overlayBorder
        Behavior on height { NumberAnimation { duration: Theme.colorDuration } }
        Rectangle {
            x: slider.mirrored ? parent.width - width : 0
            width: slider.position * parent.width
            anchors.verticalCenter: parent.verticalCenter
            height: slider.progressThickness
            radius: height / 2
            color: Theme.textPrimary
            Behavior on height { NumberAnimation { duration: Theme.colorDuration } }
        }
    }
    handle: Item {
        x: slider.leftPadding + slider.visualPosition * (slider.availableWidth - width)
        y: slider.topPadding + slider.availableHeight / 2 - height / 2
        implicitWidth: slider.thumbSize
        implicitHeight: slider.thumbSize
        visible: slider.thumbVisible || slider.pressed || slider.visualFocus
        // Animate the drawing without changing the thumb's travel or hit area.
        Rectangle {
            id: thumb
            objectName: "seekThumb"
            anchors.centerIn: parent
            anchors.alignWhenCentered: false
            width: slider.emphasized ? Theme.seekThumbActiveSize : slider.thumbSize
            height: width
            radius: width / 2
            antialiasing: true
            color: Theme.textPrimary
            Behavior on width { NumberAnimation { duration: Theme.colorDuration; easing.type: Easing.OutCubic } }
        }
        Rectangle {
            objectName: "seekFocusRing"
            anchors.centerIn: parent
            anchors.alignWhenCentered: false
            width: thumb.width + 2 * (Theme.seekFocusGap + Theme.seekFocusWidth)
            height: width
            radius: width / 2
            antialiasing: true
            color: "transparent"
            border.width: Theme.seekFocusWidth
            border.color: Theme.accent
            border.pixelAligned: false
            opacity: slider.enabled && slider.visualFocus ? 1 : 0
            Behavior on opacity { NumberAnimation { duration: Theme.fadeInDuration } }
        }
    }
    Label {
        id: positionLabel
        objectName: "seekPositionLabel"
        visible: slider.positionLabelVisible
        readonly property real fraction: Math.max(0, Math.min(1,
            (slider.indicatedValue - slider.from) / Math.max(1, slider.to - slider.from)))
        readonly property real visualFraction: slider.mirrored ? 1 - fraction : fraction
        x: Math.max(0, Math.min(slider.width - width, slider.leftPadding + slider.thumbSize / 2
            + visualFraction * (slider.availableWidth - slider.thumbSize) - width / 2))
        y: -height - Theme.spaceSm
        text: slider.positionText
        textFormat: Text.PlainText
        color: Theme.textPrimary
        font.pixelSize: Theme.fontControl
        font.family: "monospace"
        padding: Theme.spaceSm
        background: Rectangle {
            color: Theme.overlaySurface
            radius: Theme.controlRadius
            border.color: Theme.overlayBorder
        }
    }
}
