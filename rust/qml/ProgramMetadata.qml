pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls

Column {
    id: root
    required property var program
    readonly property var sections: program && program.extended ? program.extended : []
    readonly property var broadcastFields: fields()
    spacing: 22
    AudioLabels { id: audioLabels }

    function fields() {
        if (!program) return []
        const result = []
        if (program.series) {
            const series = program.series
            const parts = series.name ? [series.name] : []
            if (series.episode > 0) parts.push(qsTranslate("Viewer", "Episode %1").arg(series.episode))
            if (series.lastEpisode > 0) parts.push(qsTranslate("Viewer", "%1 episodes").arg(series.lastEpisode))
            if (parts.length) result.push({heading:qsTranslate("Viewer", "Series"), text:parts.join(" · ")})
        }
        if (program.video) {
            const codecs = {mpeg2:"MPEG-2", "h.264":"H.264", "h.265":"H.265"}
            const video = program.video
            const parts = []
            if (video.resolution) {
                const labels = {"480i":"SD", "480p":"SD", "720p":"HD", "1080i":"HD", "1080p":"Full HD", "2160p":"4K", "4320p":"8K"}
                parts.push((labels[video.resolution] ? labels[video.resolution] + " · " : "") + video.resolution)
            }
            if (video.type) parts.push(codecs[video.type] || video.type)
            if (parts.length) result.push({heading:qsTranslate("Viewer", "Video format"), text:parts.join(" / ")})
        }
        for (const audio of program.audios || []) {
            const heading = audio.isMain ? qsTranslate("Main", "Main audio") : qsTranslate("Main", "Sub audio")
            const text = audioLabels.details(audio)
            if (text) result.push({heading:heading, text:text})
        }
        return result
    }
    Repeater {
        model: root.sections
        delegate: Column {
            id: section
            required property var modelData
            required property int index
            objectName: "programExtendedSection" + index
            width: root.width
            visible: modelData.heading.length > 0 || modelData.text.length > 0
            spacing: 7
            Label {
                width: parent.width
                visible: text.length > 0
                text: section.modelData.heading
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
                color: "#b7cdbd"; font.pixelSize: 14; font.bold: true
            }
            Label {
                objectName: "programExtendedText" + section.index
                width: parent.width
                text: section.modelData.text
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
                color: "#e0e5e0"; font.pixelSize: 14; lineHeight: 1.25
            }
        }
    }
    Rectangle {
        objectName: "programBroadcastInfo"
        width: root.width
        implicitHeight: broadcast.implicitHeight + 32
        visible: root.broadcastFields.length > 0
        radius: 12
        color: "#1e2821"
        Column {
            id: broadcast
            x: 16; y: 16
            width: parent.width - 32
            spacing: 14
            Label {
                width: parent.width
                text: qsTranslate("Viewer", "Broadcast information")
                wrapMode: Text.Wrap
                color: "#b7cdbd"; font.pixelSize: 13; font.bold: true
            }
            Repeater {
                model: root.broadcastFields
                delegate: Column {
                    id: field
                    required property var modelData
                    required property int index
                    objectName: "programBroadcastField" + index
                    width: broadcast.width
                    spacing: 4
                    Label {
                        width: parent.width
                        text: field.modelData.heading; textFormat: Text.PlainText
                        color: "#a4b3a8"; font.pixelSize: 12; wrapMode: Text.Wrap
                    }
                    Label {
                        objectName: "programBroadcastValue" + field.index
                        width: parent.width
                        text: field.modelData.text; textFormat: Text.PlainText
                        color: "#e0e5e0"; font.pixelSize: 14; wrapMode: Text.Wrap
                    }
                }
            }
        }
    }
}
