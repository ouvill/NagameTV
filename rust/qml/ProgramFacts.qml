pragma ComponentBehavior: Bound
import QtQuick
import MinimalViewer
import QtQuick.Controls

Flow {
    id: root
    required property var program
    required property double now
    enum BroadcastState { Upcoming, OnAir, Finished }
    readonly property int broadcastState: !program || now < program.startAt ? ProgramFacts.Upcoming
        : now < program.startAt + program.duration ? ProgramFacts.OnAir : ProgramFacts.Finished
    readonly property var facts: labels()
    readonly property int minuteMs: 60000
    spacing: Theme.spaceSm

    function labels() {
        if (!program) return []
        const result = []
        switch (broadcastState) {
        case ProgramFacts.Upcoming: result.push(qsTranslate("Viewer", "Upcoming")); break
        case ProgramFacts.OnAir: result.push(qsTranslate("Viewer", "On air")); break
        case ProgramFacts.Finished: result.push(qsTranslate("Viewer", "Ended")); break
        }
        if (program.duration > 0) result.push(qsTranslate("Viewer", "%1 min").arg(Math.ceil(program.duration / minuteMs)))
        const genres = [
            qsTranslate("Viewer", "News / Reports"), qsTranslate("Viewer", "Sports"),
            qsTranslate("Viewer", "Information / Lifestyle"), qsTranslate("Viewer", "Drama"),
            qsTranslate("Viewer", "Music"), qsTranslate("Viewer", "Variety"),
            qsTranslate("Viewer", "Film"), qsTranslate("Viewer", "Animation / Special effects"),
            qsTranslate("Viewer", "Documentary / Culture"), qsTranslate("Viewer", "Theater / Performance"),
            qsTranslate("Viewer", "Hobbies / Education"), qsTranslate("Viewer", "Welfare")
        ]
        if (typeof program.genre === "number" && genres[program.genre]) result.push(genres[program.genre])
        if (typeof program.isFree === "boolean") result.push(program.isFree
            ? qsTranslate("Viewer", "Free-to-air") : qsTranslate("Viewer", "Paid broadcast"))
        return result
    }
    Repeater {
        model: root.facts
        delegate: Rectangle {
            id: fact
            required property string modelData
            required property int index
            objectName: "programFact" + index
            width: Math.min(root.width, label.implicitWidth + 20)
            height: label.implicitHeight + 10
            radius: Theme.controlRadius
            color: index === 0 && root.broadcastState === ProgramFacts.OnAir ? Theme.surfaceSelected : Theme.surfaceRaised
            Label {
                id: label
                x: 10; y: 5; width: parent.width - 20
                text: fact.modelData; textFormat: Text.PlainText
                color: fact.index === 0 && root.broadcastState === ProgramFacts.OnAir ? Theme.textSecondary : Theme.textSecondary
                font.pixelSize: Theme.fontCaption; wrapMode: Text.Wrap
            }
        }
    }
}
