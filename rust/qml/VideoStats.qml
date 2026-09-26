pragma ComponentBehavior: Bound
import QtQuick
import MinimalViewer
import QtQuick.Controls
import QtQuick.Layouts

Rectangle {
    id: panel
    required property var backend
    required property size viewportSize
    required property real viewportDpr
    property url closeIcon: "../assets/icons/x.svg"
    signal closeRequested
    property var snapshot: ({})
    color: Theme.overlaySurface
    border.color: Theme.overlayBorder
    radius: Theme.panelRadius
    implicitHeight: content.implicitHeight + 28
    function number(value, digits) { return typeof value === "number" && isFinite(value) ? value.toFixed(digits) : "—" }
    function formatVideo(format) {
        return format && format.width && format.height
            ? format.width + " × " + format.height + " / " + number(format.fps, 3) + " fps" : "—"
    }
    function pixels(format) {
        if (!format?.pixel_format) return "—"
        const memory = format.memory?.replace("memory:", "")
        return format.pixel_format + (memory ? " (" + memory + ")" : "")
    }
    function scan(format) {
        switch (format?.interlace) {
        case "progressive": return qsTranslate("Main", "Progressive")
        case "interleaved": return qsTranslate("Main", "Interlaced")
        case "fields": return qsTranslate("Main", "Interlaced (separate fields)")
        case "alternate": return qsTranslate("Main", "Interlaced (alternate fields)")
        case "mixed":
            switch (format.scan) {
            case "interlaced": return qsTranslate("Main", "Mixed · latest input: interlaced")
            case "progressive": return qsTranslate("Main", "Mixed · latest input: progressive")
            default: return qsTranslate("Main", "Mixed · waiting for a frame")
            }
        default: return "—"
        }
    }
    function deinterlacing(stats) {
        switch (stats.deinterlace_status) {
        case "active": return qsTranslate("Main", "Active: %1").arg(stats.deinterlacer || "—")
        case "passthrough": return qsTranslate("Main", "Not applied (passthrough)")
        case "disabled": return qsTranslate("Main", "Disabled")
        default: return "—"
        }
    }
    function metric(key) {
        const s = panel.snapshot
        switch (key) {
        case "state": return s.state ? qsTranslate("Backend", s.state) : "—"
        case "input": return panel.formatVideo(s.input)
        case "scan": return panel.scan(s.input) + " / " + (s.input?.pixel_aspect_ratio || "—")
        case "viewport": return Math.round(panel.viewportSize.width) + " × " + Math.round(panel.viewportSize.height) + " / " + panel.viewportDpr
        case "output": return panel.formatVideo(s.output)
        case "pixels": return panel.pixels(s.input) + " → " + panel.pixels(s.output)
        case "deinterlace": return panel.deinterlacing(s)
        case "deinterlaceSetting": return s.deinterlacer || "—"
        case "rate": return panel.number(s.average_fps, 2) + " fps"
        case "frames": return panel.number(s.rendered, 0) + " / " + panel.number(s.dropped, 0)
        case "queue": return panel.number(s.queue_buffers, 0) + " frames / " + panel.number(s.queue_ms, 1) + " ms"
        case "memory": return panel.number(s.queue_bytes / 1048576, 2) + " MiB"
        case "engine": return (s.gstreamer || "—") + (s.decoders?.length ? " / " + s.decoders.join(", ") : "")
        default: return "—"
        }
    }
    // Loader destroys this timer and its only snapshot when the panel closes.
    Timer {
        interval: 1000; repeat: true; running: panel.visible; triggeredOnStart: true
        onTriggered: panel.snapshot = JSON.parse(panel.backend.video_stats())
    }
    MouseArea { anchors.fill: parent; acceptedButtons: Qt.AllButtons }
    ColumnLayout {
        id: content
        anchors { left: parent.left; right: parent.right; top: parent.top; margins: 14 }
        spacing: 7
        RowLayout {
            Layout.fillWidth: true
            Label { text: qsTranslate("Main", "Stats for nerds"); font.pixelSize: Theme.fontBody; font.bold: true; color: Theme.textPrimary; Layout.fillWidth: true }
            IconAction {
                iconSource: panel.closeIcon
                tip: qsTranslate("Main", "Close stats for nerds")
                onClicked: panel.closeRequested()
            }
        }
        Repeater {
            // Constant rows: replace values, not delegate objects, on each sample.
            model: [
                [qsTranslate("Main", "State"), "state"], [qsTranslate("Main", "Input video"), "input"], [qsTranslate("Main", "Input scan / PAR"), "scan"],
                [qsTranslate("Main", "Output video"), "output"], [qsTranslate("Main", "Pixels: input → output"), "pixels"],
                [qsTranslate("Main", "Viewport / DPR"), "viewport"], [qsTranslate("Main", "Deinterlacing"), "deinterlaceSetting"],
                [qsTranslate("Main", "Applied deinterlacing"), "deinterlace"], [qsTranslate("Main", "Sink average rate"), "rate"],
                [qsTranslate("Main", "Sink rendered / dropped"), "frames"], [qsTranslate("Main", "Video queue"), "queue"],
                [qsTranslate("Main", "Queue memory"), "memory"], [qsTranslate("Main", "Playback engine"), "engine"]
            ]
            delegate: RowLayout {
                id: metricRow
                required property var modelData
                Layout.fillWidth: true
                Label { text: metricRow.modelData[0]; color: Theme.textSecondary; Layout.preferredWidth: 142; font.pixelSize: Theme.fontCaption; wrapMode: Text.Wrap }
                Label { text: panel.metric(metricRow.modelData[1]); color: Theme.textPrimary; Layout.fillWidth: true; wrapMode: Text.Wrap; font.pixelSize: Theme.fontCaption }
            }
        }
        Label {
            text: qsTranslate("Main", "Updated every second. Sink frame counts do not measure actual screen presentations. Queue time is not live latency.")
            color: Theme.textSecondary; font.pixelSize: Theme.fontMicro; wrapMode: Text.Wrap; Layout.fillWidth: true
        }
    }
}
