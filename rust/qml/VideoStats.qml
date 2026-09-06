import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Rectangle {
    id: panel
    required property var backend
    property var snapshot: ({})
    color: "#e820252c"
    border.color: "#6b7582"
    radius: 6
    implicitHeight: content.implicitHeight + 24
    function number(value, digits) { return typeof value === "number" && isFinite(value) ? value.toFixed(digits) : "—" }
    function video(format) {
        return format && format.width && format.height
            ? format.width + " × " + format.height + " / " + number(format.fps, 3) + " fps" : "—"
    }
    function metric(key) {
        const s = panel.snapshot
        switch (key) {
        case "state": return s.state || "—"
        case "input": return panel.video(s.input)
        case "scan": return (s.input?.interlace || "—") + " / " + (s.input?.pixel_aspect_ratio || "—")
        case "output": return panel.video(s.output)
        case "pixels": return (s.input?.pixel_format || "—") + " → " + (s.output?.pixel_format || "—")
        case "deinterlace": return s.deinterlacer || "—"
        case "rate": return panel.number(s.average_fps, 2)
        case "frames": return panel.number(s.rendered, 0) + " / " + panel.number(s.dropped, 0)
        case "queue": return panel.number(s.queue_buffers, 0) + " frames / " + panel.number(s.queue_ms, 1) + " ms"
        case "memory": return panel.number(s.queue_bytes / 1048576, 2) + " MiB"
        case "engine": return s.gstreamer || "—"
        default: return "—"
        }
    }
    // Loader destroys this timer and its only snapshot when the panel closes.
    Timer {
        interval: 1000; repeat: true; running: panel.visible; triggeredOnStart: true
        onTriggered: panel.snapshot = JSON.parse(panel.backend.video_stats())
    }
    ColumnLayout {
        id: content
        anchors { left: parent.left; right: parent.right; top: parent.top; margins: 12 }
        Label { text: qsTranslate("Main", "Stats for nerds"); font.bold: true; color: "white" }
        Repeater {
            // Constant rows: replace values, not delegate objects, on each sample.
            model: [
                [qsTranslate("Main", "State"), "state"], ["入力", "input"], ["走査 / PAR", "scan"],
                ["出力", "output"], ["画素形式", "pixels"],
                ["デインターレース", "deinterlace"], ["sink平均fps", "rate"],
                ["sink描画 / 破棄", "frames"], ["キュー", "queue"],
                ["キューメモリー", "memory"], ["エンジン", "engine"]
            ]
            delegate: RowLayout {
                required property var modelData
                Layout.fillWidth: true
                Label { text: modelData[0]; color: "#b8c4d4"; Layout.preferredWidth: 128; font.pixelSize: 12 }
                Label { text: panel.metric(modelData[1]); color: "white"; Layout.fillWidth: true; wrapMode: Text.Wrap; font.pixelSize: 12 }
            }
        }
        Label {
            text: "sink集計は画面の実表示回数、キュー時間は放送からの遅延とは異なります。"
            color: "#b8c4d4"; font.pixelSize: 11; wrapMode: Text.Wrap; Layout.fillWidth: true
        }
    }
}
