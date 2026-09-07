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
            iconDirectory: Qt.resolvedUrl("../../../assets/icons/")
            programsJson: "[]"
            channel: "Test channel"
            status: "Ready"
            property var requests: []
            onDayRequested: function(start, end) { requests.push([start,end]) }
        }
    }
    property var guide
    SignalSpy { id: settingsSpy; target: testCase.guide || null; signalName: "settingsRequested" }
    SignalSpy { id: closeSpy; target: testCase.guide || null; signalName: "closeRequested" }
    SignalSpy { id: watchSpy; target: testCase.guide || null; signalName: "watchRequested" }
    function initTestCase() { failOnWarning(/.*/) }
    function init() {
        failOnWarning(/.*/)
        testCase.Window.window.width = 640; testCase.Window.window.height = 480
        testCase.width = 640; testCase.height = 480
        guide = createTemporaryObject(component, testCase)
        verify(guide !== null)
    }
    function test_open_details_follow_identity_across_epg_refresh() {
        guide.dayOffset = 1
        const start = guide.days[1].start
        guide.rows = [{index:0, label:"Channel 0", band:"GR", logo:""}]
        const original = {watchKey:"selected", name:"Before", description:"Old description", startAt:start, duration:3600000}
        const other = {watchKey:"other", name:"Other", description:"Other description", startAt:start+3600000, duration:3600000}
        guide.programsJson = JSON.stringify([{index:0, programs:[original, other]}])
        verify(waitForRendering(guide))
        const cell = findChild(guide, "guideCell")
        verify(cell !== null)
        mouseClick(cell, 20, 30)
        const loader = findChild(guide, "scheduledDetailsLoader")
        tryCompare(loader.item, "opened", true)
        compare(findChild(loader.item, "programTitle").text, "Before")
        const updated = Object.assign({}, original, {name:"After", description:"New description"})
        guide.programsJson = JSON.stringify([{index:0, programs:[other, updated]}])
        tryCompare(findChild(loader.item, "programTitle"), "text", "After")
        compare(findChild(loader.item, "programDescription").text, "New description")
        compare(guide.selectedProgram.duration, 3600000)
        compare(guide.selectedProgram.watchKey, "selected")
        // Removing the selected identity must not display its former row's replacement.
        guide.programsJson = JSON.stringify([{index:0, programs:[other]}])
        tryCompare(loader, "item", null)
        compare(guide.selectedProgram, null)
    }
    function test_detail_position_resize_and_animated_dismissal() {
        testCase.Window.window.width = 1440; testCase.Window.window.height = 900
        testCase.width = 1440; testCase.height = 900
        guide.width = 1440; guide.height = 880
        verify(waitForRendering(guide))
        guide.selectedPosition = Qt.point(104, 140)
        guide.selectedChannel = "101 NHK BS"
        guide.selectedProgram = {name:"World news",description:"Description",startAt:0,duration:1}
        const loader = findChild(guide, "scheduledDetailsLoader")
        const popup = loader.item
        tryCompare(popup, "opened", true)
        compare(popup.width, 500); compare(popup.height, 360)
        compare(popup.x, 350); compare(popup.y, 140)
        compare(popup.dim, false)
        compare(findChild(popup, "programChannel").text, "101 NHK BS")
        guide.selectedPosition = Qt.point(1100, 1000)
        compare(popup.x, 572)
        compare(popup.y, popup.parent.height - 380)
        guide.width = 640; guide.height = 480
        verify(popup.x + popup.width <= guide.width - 24)
        verify(popup.y + popup.height <= popup.parent.height - 20)
        keyClick(Qt.Key_Escape)
        // The loader must survive until the exit transition completes.
        verify(loader.item !== null)
        tryCompare(loader, "item", null)
        compare(guide.selectedProgram, null)
    }
    function test_visible_channel_indices_preserve_catalog_identity_and_clear_details() {
        guide.rows = [
            {index: 0, band: "GR", label: "Primary"},
            {index: 1, band: "GR", label: "Simulcast"},
            {index: 2, band: "GR", label: "Independent"},
            {index: 3, band: "BS", label: "Satellite"}
        ]
        guide.band = "GR"
        compare(guide.visibleRows.length, 3)
        guide.visibilityJson = "[0,2,3]"
        const timeline = findChild(guide, "guideTimeline").parent
        compare(timeline.rows.length, 2)
        compare(timeline.rows[1].index, 2)
        guide.selectedProgram = {name:"Old details", startAt:0, duration:1}
        guide.visibilityJson = "[0,1,2,3]"
        compare(guide.selectedProgram, null)
        compare(timeline.rows.length, 3)
        guide.band = "BS"
        compare(timeline.rows.length, 1)
        compare(timeline.rows[0].index, 3)
        guide.visibilityJson = "[]"
        compare(timeline.rows.length, 0)
    }
    function test_current_time_badge_tracks_scrolling_and_selected_day() {
        const view = findChild(guide, "guideTimeline")
        const timeline = view.parent
        const badge = findChild(timeline, "guideCurrentTimeBadge")
        const line = findChild(timeline, "guideCurrentTimeLine")
        timeline.now = timeline.dayStart + 12 * 3600000
        view.contentY = timeline.currentTimeY - 200
        verify(badge.visible)
        verify(line.visible)
        compare(badge.y + badge.height / 2, 200)
        compare(badge.mapToItem(timeline, 0, badge.height / 2).y, line.mapToItem(timeline, 0, 0).y)
        view.contentY += 50
        compare(badge.y + badge.height / 2, 150)
        compare(badge.mapToItem(timeline, 0, badge.height / 2).y, line.mapToItem(timeline, 0, 0).y)
        timeline.now = timeline.dayEnd
        verify(!badge.visible)
        verify(!line.visible)
        timeline.now = timeline.dayStart - 1
        verify(!badge.visible)
    }
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
    function test_toolbar_forwards_actions_and_reserves_main_height() {
        settingsSpy.clear()
        closeSpy.clear()
        const toolbar = findChild(guide, "guideToolbar")
        compare(toolbar.height, 84)
        compare(findChild(guide, "guideTimeline").parent.y, 84)
        verify(waitForRendering(toolbar))
        mouseClick(findChild(toolbar, "guideSettings"))
        compare(settingsSpy.count, 1)
        mouseClick(findChild(toolbar, "closeGuide"))
        compare(closeSpy.count, 1)
    }
    function test_watch_key_is_unchanged_and_expired_program_cannot_request() {
        watchSpy.clear()
        const key = '{"endpoint":18446744073709551615}'
        const start = Date.now() - 1000
        guide.selectedProgram = {name:"Live",description:"Description",startAt:start,duration:60000,watchKey:key}
        const popup = findChild(guide, "scheduledDetailsLoader").item
        tryCompare(popup, "opened", true)
        const button = findChild(popup, "watchGuideProgram")
        verify(button.visible)
        mouseClick(button)
        compare(watchSpy.count, 1)
        compare(watchSpy.signalArguments[0][0], key)
        guide.watchError = "番組情報が更新されています"
        compare(findChild(popup, "watchGuideError").text, guide.watchError)
        guide.selectedProgram = {name:"長い番組名の表示確認 ".repeat(30),description:"Description",startAt:start,duration:60000,watchKey:key}
        guide.watchError = "番組情報が更新されています。番組表から選び直してください"
        verify(waitForRendering(guide))
        verify(button.y + button.height <= popup.availableHeight)
        popup.now = start + 60000
        compare(button.visible, false)
        popup.now = start - 1
        compare(button.visible, false)
    }
    function test_details_allow_toolbar_and_outside_click_dismisses() {
        settingsSpy.clear()
        guide.selectedProgram = {name:"Program",description:"Description",startAt:0,duration:1}
        const loader = findChild(guide, "scheduledDetailsLoader")
        tryCompare(loader.item, "opened", true)
        mouseClick(findChild(guide, "guideSettings"))
        compare(settingsSpy.count, 1)
        tryCompare(loader, "item", null)
        guide.selectedProgram = {name:"Program",description:"Description",startAt:0,duration:1}
        tryCompare(loader.item, "opened", true)
        mouseClick(guide, 10, 150)
        tryCompare(loader, "item", null)
    }
    function test_shift_wheel_steps_columns_and_normal_wheel_stays_vertical() {
        guide.dayOffset = 1
        const rows = []
        for (let i = 0; i < 8; ++i) rows.push({index:i,label:"Channel " + i,band:"GR",logo:""})
        guide.rows = rows
        const view = findChild(guide, "guideTimeline")
        const wheel = findChild(guide, "guideWheelArea")
        verify(waitForRendering(guide))
        mouseWheel(wheel, 20, 130, 0, -120, Qt.NoButton, Qt.ShiftModifier)
        tryCompare(view, "contentX", 222)
        compare(view.contentY, 0)
        mouseWheel(wheel, 20, 130, 0, -120, Qt.NoButton, Qt.NoModifier)
        tryVerify(function() { return view.contentY > 0 })
        compare(view.contentX, 222)
        mouseWheel(wheel, 20, 130, 0, 120, Qt.NoButton, Qt.ShiftModifier)
        tryCompare(view, "contentX", 0)
        view.contentX = view.contentWidth - view.width
        const end = view.contentX
        mouseWheel(wheel, 20, 130, 0, -120, Qt.NoButton, Qt.ShiftModifier)
        wait(200)
        compare(view.contentX, end)
        mouseWheel(wheel, 20, 130, 0, 120, Qt.NoButton, Qt.ShiftModifier)
        guide.band = "BS"
        wait(200)
        compare(view.contentX, 0)
    }
    function test_compact_boundaries_and_wide_date_click() {
        const selector = findChild(guide, "guideDay")
        const previous = findChild(selector, "previousGuideDay")
        const next = findChild(selector, "nextGuideDay")
        compare(selector.compact, true)
        compare(previous.enabled, false)
        verify(waitForRendering(selector))
        mouseClick(next)
        compare(guide.dayOffset, 1)
        guide.dayOffset = 6
        compare(next.enabled, false)
        mouseClick(previous)
        compare(guide.dayOffset, 5)
        selector.compact = false
        guide.dayOffset = 0
        verify(waitForRendering(selector))
        wait(200)
        const dateFlick = findChild(selector, "guideDateFlick")
        dateFlick.contentX = dateFlick.contentWidth - dateFlick.width
        mouseClick(findChild(selector, "guideDate6"))
        compare(guide.dayOffset, 6)
        compare(guide.requests[guide.requests.length - 1][0], guide.days[6].start)
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
        compare(guide.selectedChannel, "Channel 0")
        compare(guide.selectedPosition.x, 104)
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
