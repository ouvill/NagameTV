import QtQuick
import QtTest
import ".." as Viewer

TestCase {
    id: testCase
    name: "ProgramGuide"
    when: windowShown
    visible: true
    width: 640; height: 480
    Component {
        id: component
        Viewer.ProgramGuide {
            width: 600; height: 460
            programsJson: "[]"
            channel: "Test channel"
            status: "Ready"
            property var requests: []
            onDayRequested: function(start, end) { requests.push([start,end]) }
        }
    }
    property var guide
    function initTestCase() { failOnWarning(/.*/) }
    function init() { failOnWarning(/.*/); guide = createTemporaryObject(component, testCase); verify(guide !== null) }
    function test_seven_calendar_days_and_selection() {
        compare(guide.days.length, 7)
        verify(guide.requests.length > 0)
        const selector = findChild(guide, "guideDay")
        selector.forceActiveFocus()
        keyClick(Qt.Key_Down)
        compare(guide.dayOffset, 1)
        const last = guide.requests[guide.requests.length-1]
        compare(last[0], guide.days[1].start)
        compare(last[1], guide.days[1].end)
        guide.dayOffset = 6
        compare(selector.currentIndex, 6)
        compare(guide.requests[guide.requests.length-1][0], guide.days[6].start)
    }
    function test_calendar_days_follow_local_midnight_through_dst() {
        const january = new Date(2026,0,1).getTimezoneOffset()
        const july = new Date(2026,6,1).getTimezoneOffset()
        for (const month of [2,10]) {
            const day = month === 2 ? 8 : 1
            const days = guide.calendarDays(new Date(2026,month,day).getTime())
            for (let i = 0; i < 7; ++i) {
                compare(new Date(days[i].start).getHours(), 0)
                compare(new Date(days[i].end).getHours(), 0)
                if (i) compare(days[i-1].end, days[i].start)
            }
            // The suite is also run with TZ=America/New_York, exercising 23/25h.
            if (january === 300 && july === 240)
                compare((days[0].end-days[0].start)/3600000, month === 2 ? 23 : 25)
        }
    }
    function test_scheduled_details_survive_delegate_disposal_and_close_on_day_change() {
        guide.dayOffset = 1
        const rows = []
        const columns = []
        for (let i=0;i<30;++i) {
            rows.push({ index:i, label:"Channel "+i, band:"GR", logo:"" })
            columns.push({index:i, programs:[{id:i,genre:0,name:"Program "+i,description:"Full description "+i,startAt:guide.days[1].start,duration:3600000}]})
        }
        guide.rows = rows
        guide.programsJson = JSON.stringify(columns)
        const list = findChild(guide, "guideTimeline")
        verify(waitForRendering(guide))
        verify(findChild(guide, "guideColumn0").item !== null)
        compare(findChild(guide, "guideColumn29").item, null)
        const first = findChild(guide, "guideCell")
        verify(first !== null)
        compare(first.color, "#ffffe0")
        compare(list.parent.genreColor(15), "#f0f0f0")
        compare(list.parent.genreColor(undefined), "#f0f0f0")
        compare(list.parent.genreColor(null), "#f0f0f0")
        mouseClick(first, 20, 30)
        const loader = findChild(guide, "scheduledDetailsLoader")
        verify(loader.item !== null)
        tryCompare(loader.item, "opened", true)
        compare(findChild(loader.item, "programDescription").text, "Full description 0")
        list.contentX = list.contentWidth - list.width
        verify(waitForRendering(guide))
        compare(findChild(guide, "guideColumn0").item, null)
        verify(findChild(guide, "guideColumn29").item !== null)
        compare(findChild(loader.item, "programTitle").text, "Program 0")
        guide.dayOffset = 2
        compare(loader.item, null)
        compare(guide.selectedProgram, null)
    }
}
