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
    function init() { guide = createTemporaryObject(component, testCase); verify(guide !== null) }
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
        const entries = []
        for (let i=0;i<300;++i) entries.push({id:i,name:"Program "+i,description:"Full description "+i,startAt:guide.days[0].start+i*60000,duration:60000})
        guide.programsJson = JSON.stringify(entries)
        const list = findChild(guide, "guidePrograms")
        verify(waitForRendering(guide))
        compare(list.count, 300)
        verify(list.contentItem.children.length < 100, "all delegates were created")
        const first = list.itemAtIndex(0)
        verify(first !== null)
        mouseClick(first)
        const loader = findChild(guide, "scheduledDetailsLoader")
        verify(loader.item !== null)
        tryCompare(loader.item, "opened", true)
        compare(findChild(loader.item, "programDescription").text, "Full description 0")
        list.positionViewAtEnd()
        verify(waitForRendering(guide))
        compare(findChild(loader.item, "programTitle").text, "Program 0")
        guide.dayOffset = 1
        compare(loader.item, null)
        compare(guide.selectedProgram, null)
    }
}
