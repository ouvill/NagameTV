import QtQuick
import QtTest
import MinimalViewer as Viewer

// Frame pacing while moving the production guide through a cached schedule.
// Run through scripts/run-gui-tests.py; no network or physical input required.
TestCase {
    id: testCase
    name: "GuideScrolling"
    // Finish measuring before assertions, with no QtTest polling waits inside
    // the measured interval.
    when: windowShown && phase === GuideScrolling.Complete
    visible: true
    width: 1920; height: 1080
    Viewer.ChannelModel { id: channels }
    Viewer.GuideModel { id: schedules }
    Window { id: guideWindow }
    property var guide: null
    property var intervals: []
    enum Phase { Idle, Warming, Forward, Reverse, Complete }
    property int phase: GuideScrolling.Idle
    readonly property var directions: ["vertical", "horizontal", "diagonal"]
    property int direction: 0
    property var results: []
    property string error: ""
    property real extentX: 0
    property real extentY: 0
    readonly property int traversalDuration: 5000
    onWindowShownChanged: if (windowShown && phase === GuideScrolling.Idle) prepare()
    Timer { id: warmup; interval: 200; onTriggered: testCase.startForward() }
    Timer {
        interval: 45000
        running: testCase.phase !== GuideScrolling.Idle && testCase.phase !== GuideScrolling.Complete
        onTriggered: {
            clock.stop(); motion.stop()
            testCase.error = "Scroll benchmark timed out"
            testCase.phase = GuideScrolling.Complete
        }
    }
    FrameAnimation {
        id: clock
        onTriggered: if (currentFrame > 1) testCase.intervals.push(frameTime * 1000)
    }
    ParallelAnimation {
        id: motion
        NumberAnimation { id: horizontal; property: "contentX"; duration: testCase.traversalDuration; easing.type: Easing.Linear }
        NumberAnimation { id: vertical; property: "contentY"; duration: testCase.traversalDuration; easing.type: Easing.Linear }
        onFinished: testCase.finishMotion()
    }
    Component {
        id: component
        Viewer.ProgramGuide {
            width: testCase.width; height: testCase.height
            guideModel: schedules; channels: testCase.channelModel
            targetWindow: guideWindow
            status: "Ready"; channel: "Benchmark"; selected: 0; dayOffset: 1
        }
    }
    readonly property alias channelModel: channels
    function prepare() {
        failOnWarning(/.*/)
        phase = GuideScrolling.Warming
        testCase.Window.window.width = width
        testCase.Window.window.height = height
        const start = new Date(); start.setDate(start.getDate()+1); start.setHours(0,0,0,0)
        const count = 54, duration = 30*60000
        const validChannels = channels.load_test(JSON.stringify(Array.from({length:count}, (_,i) =>
            ({index:i, label:"Channel "+i, band:"GR", logo:""}))))
        const validSchedules = schedules.load_test(JSON.stringify(Array.from({length:count}, (_,channel) =>
            ({index:channel, programs:Array.from({length:48}, (_,i) =>
                ({watchKey:channel+"/"+i, startAt:start.getTime()+i*duration, duration:duration,
                    name:"番組タイトル "+i+"　放送予定の説明", genre:i%12,
                    description:"番組の概要と出演者などの説明文です。".repeat(20)}))}))))
        if (!validChannels || !validSchedules) {
            error = "Invalid scroll fixture"
            phase = GuideScrolling.Complete
            return
        }
        guide = component.createObject(testCase)
        if (!guide) {
            error = "Could not create the guide"
            phase = GuideScrolling.Complete
            return
        }
        warmup.start()
    }
    function startForward() {
        const view = findChild(guide,"guideTimeline")
        horizontal.target = view
        vertical.target = view
        extentX = directions[direction] !== "vertical" ? Math.min(4000, view.contentWidth-view.width) : 0
        extentY = directions[direction] !== "horizontal" ? view.contentHeight-view.height : 0
        intervals = []
        phase = GuideScrolling.Forward
        horizontal.from = 0; horizontal.to = extentX
        vertical.from = 0; vertical.to = extentY
        clock.restart()
        motion.start()
    }
    function finishMotion() {
        clock.stop()
        if (!checkViewport()) {
            phase = GuideScrolling.Complete
            return
        }
        if (phase === GuideScrolling.Forward) {
            phase = GuideScrolling.Reverse
            horizontal.from = extentX; horizontal.to = 0
            vertical.from = extentY; vertical.to = 0
            clock.restart()
            motion.start()
            return
        }
        const sorted = intervals.slice().sort((a,b) => a-b)
        results.push({direction:directions[direction],
            frames:sorted.length, median:sorted[Math.floor(sorted.length/2)],
            p95:sorted[Math.floor(sorted.length*0.95)], p99:sorted[Math.floor(sorted.length*0.99)],
            max:sorted[sorted.length-1], over25ms:sorted.filter(x=>x>25).length,
            over50ms:sorted.filter(x=>x>50).length})
        direction++
        if (direction === directions.length) phase = GuideScrolling.Complete
        else { phase = GuideScrolling.Warming; warmup.start() }
    }
    function checkViewport() {
        const view = findChild(guide,"guideTimeline")
        const timeline = view.parent
        const first = Math.floor(view.contentX/timeline.channelWidth)
        const last = Math.ceil((view.contentX+view.width)/timeline.channelWidth)-1
        function checkCells(item) {
            if (item.objectName === "guideCellLoader"
                    && item.y+item.height > view.contentY+timeline.channelHeaderHeight
                    && item.y < view.contentY+view.height) {
                if (item.status !== Loader.Ready || !item.item) return -1
                return 1
            }
            let count = 0
            for (const child of item.children || []) {
                const found = checkCells(child)
                if (found < 0) return -1
                count += found
            }
            return count
        }
        for (let i=first; i<=last; ++i) {
            const column = findChild(guide,"guideColumn"+i)
            if (!column || checkCells(column) <= 0) {
                error = "Visible programs missing in column "+i
                return false
            }
        }
        return true
    }
    function test_results() {
        compare(error, "")
        compare(results.length, directions.length)
        for (const result of results) {
            verify(result.frames > 0)
            console.log("GUIDE_SCROLL_BENCH "+JSON.stringify(result))
        }
    }
}
