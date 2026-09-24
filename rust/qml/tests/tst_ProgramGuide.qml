import QtQuick
import QtTest
import MinimalViewer as Viewer

TestCase {
    id: testCase
    name: "ProgramGuide"
    when: windowShown
    visible: true
    width: 640; height: 480
    Component {
        id: component
        Viewer.ProgramGuide {
            property alias rows: fixture.rows
            channels: ChannelFixture { id: fixture }
            width: 600; height: 460
            targetWindow: guideWindow
            iconDirectory: Qt.resolvedUrl("../../../assets/icons/")
            guideModel: Viewer.GuideModel { id: schedules }
            property string programsJson: "[]"
            onProgramsJsonChanged: testCase.verify(schedules.load_test(programsJson))
            function keyFor(alias) { return schedules.test_key(alias) }
            function selectFixture(program) {
                programsJson = JSON.stringify([{index:0, programs:[program]}])
                selectedProgram = schedules.lookup(keyFor(program.watchKey || ""))
            }
            channel: "Test channel"
            status: "Ready"
            property var requests: []
            onDayRequested: function(start, end) { requests.push([start,end]) }
        }
    }
    property var guide
    Window { id: guideWindow }
    SignalSpy { id: modesSpy; target: testCase.guide || null; signalName: "modeRequested" }
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
    function test_open_reveals_current_band_and_offscreen_channel_after_catalog_arrives() {
        guide.selected = 0
        guide.viewingIndex = 23
        guide.rows = Array.from({length: 24}, (_, index) =>
            ({index:index, label:"Channel " + index, band:index % 2 ? "BS" : "GR", logo:""}))
        const view = findChild(guide, "guideTimeline")
        const timeline = view.parent
        tryCompare(guide, "band", "BS")
        tryCompare(timeline, "cursorColumn", 11)
        tryVerify(function() { return view.contentX > 0 })
        verify(timeline.cursorColumn * timeline.channelWidth >= view.contentX)
        verify((timeline.cursorColumn + 1) * timeline.channelWidth <= view.contentX + view.width + 1)
        guide.visibilityJson = "[0,1,3]"
        tryCompare(timeline, "cursorColumn", 2)
        compare(timeline.channels.row(2).channelIndex, 23)
        guide.band = "GR"
        guide.openGuide()
        tryCompare(guide, "band", "BS")
        tryCompare(timeline, "cursorColumn", 2)
        guide.viewingIndex = -1
        tryCompare(guide, "band", "GR")
        tryCompare(timeline, "cursorColumn", 0)
    }
    function prepareSwipe() {
        guide.dayOffset = 1
        guide.rows = Array.from({length:24}, (_, index) => ({index:index, label:"Channel " + index, band:"GR"}))
        verify(waitForRendering(guide))
        const view = findChild(guide, "guideTimeline")
        view.contentX = 1000
        view.contentY = 1000
        return view
    }
    function swipe(input, dx, dy) {
        const sampleIntervalMs = 16
        const sampleCount = 6
        for (let i = 0; i < sampleCount; ++i) {
            input.scroll(Qt.point(dx, dy), Qt.point(0, 0))
            wait(sampleIntervalMs)
        }
    }
    function test_touchpad_tracks_both_axes_and_coasts_data() {
        return [
            {tag:"vertical", dx:0, dy:-24},
            {tag:"horizontal", dx:-24, dy:0},
            {tag:"diagonal", dx:24, dy:-24}
        ]
    }
    function test_touchpad_tracks_both_axes_and_coasts(data) {
        const view = prepareSwipe()
        const input = findChild(guide, "guideWheelArea")
        const before = Qt.point(view.contentX, view.contentY)
        input.scroll(Qt.point(data.dx, data.dy), Qt.point(0, -120))
        compare(view.contentX, before.x - data.dx)
        compare(view.contentY, before.y - data.dy)
        swipe(input, data.dx, data.dy)
        input.finishGesture()
        verify(view.flicking)
        const released = Qt.point(view.contentX, view.contentY)
        wait(80)
        if (data.dx) verify((view.contentX - released.x) * -Math.sign(data.dx) > 0)
        else compare(view.contentX, released.x)
        if (data.dy) verify((view.contentY - released.y) * -Math.sign(data.dy) > 0)
        else compare(view.contentY, released.y)
        input.beginGesture()
        verify(!view.flicking)
        input.cancelGesture()
    }
    function test_touchpad_bounds_day_change_and_close_cancel_momentum() {
        const view = prepareSwipe()
        const input = findChild(guide, "guideWheelArea")
        view.contentX = view.contentWidth - view.width
        const rightEdge = view.contentX
        swipe(input, -24, -24)
        input.finishGesture()
        verify(view.flicking)
        wait(80)
        compare(view.contentX, rightEdge)
        guide.dayOffset = 2
        tryVerify(function() { return !view.flicking })
        input.finishGesture()
        compare(view.contentY, 0)
        swipe(input, 24, -24)
        input.finishGesture()
        verify(view.flicking)
        guide.enabled = false
        verify(!view.flicking)
        input.finishGesture()
        verify(!view.flicking)
    }
    function test_keyboard_keeps_future_time_across_empty_channel() {
        guide.dayOffset = 1
        const start = guide.days[1].start
        guide.rows = [0,1,2].map(index => ({index:index, label:"Channel " + index, band:"GR"}))
        guide.programsJson = JSON.stringify([0,1,2].map(index => ({index:index, programs:index === 1 ? [] : [
            {watchKey:"early-"+index, name:"Early", startAt:start, duration:3600000},
            {watchKey:"late-"+index, name:"Late", startAt:start+3600000, duration:3600000}
        ]})))
        verify(waitForRendering(guide))
        keyClick(Qt.Key_Down)
        keyClick(Qt.Key_Right)
        keyClick(Qt.Key_Return)
        const loader = findChild(guide,"scheduledDetailsLoader")
        compare(loader.item, null)
        keyClick(Qt.Key_Right)
        keyClick(Qt.Key_Return)
        tryCompare(loader.item, "opened", true)
        compare(guide.selectedProgram.watchKey, guide.keyFor("late-2"))
    }
    function test_keyboard_refresh_replaces_program_at_retained_time() {
        guide.dayOffset = 2
        const start = guide.days[2].start
        guide.rows = [{index:0, label:"Channel", band:"GR"}]
        const early = {watchKey:"early", name:"Early", startAt:start, duration:3600000}
        guide.programsJson = JSON.stringify([{index:0, programs:[early,
            {watchKey:"old", name:"Old", startAt:start+3600000, duration:3600000}
        ]}])
        verify(waitForRendering(guide))
        keyClick(Qt.Key_Down)
        guide.programsJson = JSON.stringify([{index:0, programs:[early,
            {watchKey:"replacement", name:"Replacement", startAt:start+3600000, duration:1800000}
        ]}])
        keyClick(Qt.Key_Return)
        const loader = findChild(guide,"scheduledDetailsLoader")
        tryCompare(loader.item, "opened", true)
        compare(guide.selectedProgram.watchKey, guide.keyFor("replacement"))
        keyClick(Qt.Key_Escape)
        tryCompare(loader, "item", null)
        // Selecting a different day must discard the previous day's time anchor.
        gc() // Rust retains the candidate model after its popup's JS wrapper is gone.
        guide.dayOffset = 1
        const next = guide.days[1].start
        guide.programsJson = JSON.stringify([{index:0, programs:[
            {watchKey:"first", name:"First", startAt:next, duration:3600000},
            {watchKey:"second", name:"Second", startAt:next+3600000, duration:3600000}
        ]}])
        keyClick(Qt.Key_Return)
        tryCompare(loader.item, "opened", true)
        compare(guide.selectedProgram.watchKey, guide.keyFor("first"))
    }
    function test_keyboard_channel_change_preserves_time_inside_long_program() {
        guide.dayOffset = 1
        const start = guide.days[1].start
        guide.rows = [{index:0, label:"Long", band:"GR"}, {index:1, label:"Short", band:"GR"}]
        guide.programsJson = JSON.stringify([
            {index:0, programs:[{watchKey:"long", name:"Long", startAt:start, duration:3*3600000}]},
            {index:1, programs:[
                {watchKey:"past", name:"Past", startAt:start, duration:3600000},
                {watchKey:"current", name:"Current", startAt:start+3600000, duration:3600000}
            ]}
        ])
        const timeline = findChild(guide, "guideTimeline").parent
        timeline.now = start + 90 * 60000
        verify(waitForRendering(guide))
        keyClick(Qt.Key_Right)
        keyClick(Qt.Key_Return)
        tryCompare(findChild(guide, "scheduledDetailsLoader").item, "opened", true)
        compare(guide.selectedProgram.watchKey, guide.keyFor("current"))
    }
    function test_keyboard_navigation_scrolls_and_reopens_details() {
        guide.dayOffset = 1
        const start = guide.days[1].start
        const rows = []
        const columns = []
        for (let i = 0; i < 8; ++i) {
            rows.push({index:i * 2, label:"Channel " + i, band:"GR", logo:""})
            columns.push({index:i * 2, programs:[
                {watchKey:"first-" + i, name:"First " + i, startAt:start, duration:3600000},
                {watchKey:"second-" + i, name:"Second " + i, startAt:start + 3600000, duration:3600000}
            ]})
        }
        guide.rows = rows
        guide.programsJson = JSON.stringify(columns)
        verify(waitForRendering(guide))
        const view = findChild(guide, "guideTimeline")
        const timeline = view.parent
        verify(timeline.activeFocus)
        keyClick(Qt.Key_Right)
        keyClick(Qt.Key_Down)
        keyClick(Qt.Key_Return)
        const loader = findChild(guide, "scheduledDetailsLoader")
        tryCompare(loader.item, "opened", true)
        compare(guide.selectedProgram.watchKey, guide.keyFor("second-1"))
        compare(guide.selectedChannel, "Channel 1")
        keyClick(Qt.Key_Escape)
        tryCompare(loader, "item", null)
        tryCompare(timeline, "activeFocus", true)
        for (let i = 0; i < 6; ++i) keyClick(Qt.Key_Right)
        keyClick(Qt.Key_Up)
        verify(view.contentX > 0)
        compare(view.contentY, 0)
        keyClick(Qt.Key_Enter, Qt.KeypadModifier)
        tryCompare(loader.item, "opened", true)
        compare(guide.selectedProgram.watchKey, guide.keyFor("first-7"))
        verify(guide.selectedPosition.x >= view.x)
        verify(guide.selectedPosition.x < timeline.width)
        keyClick(Qt.Key_Escape)
        tryCompare(loader, "item", null)
        // Replace the snapshot and remove the focused program: Enter must resolve
        // against current data, never reopen the obsolete object.
        columns[7].programs.shift()
        guide.programsJson = JSON.stringify(columns)
        keyClick(Qt.Key_Return)
        tryCompare(loader.item, "opened", true)
        compare(guide.selectedProgram.watchKey, guide.keyFor("second-7"))
        keyClick(Qt.Key_Escape)
        tryCompare(loader, "item", null)
        guide.visibilityJson = "[]"
        keyClick(Qt.Key_Return)
        compare(loader.item, null)
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
        compare(guide.selectedProgram.watchKey, guide.keyFor("selected"))
        // Removing the selected identity must not display its former row's replacement.
        guide.programsJson = JSON.stringify([{index:0, programs:[other]}])
        tryCompare(loader, "item", null)
        compare(guide.selectedProgram, null)
    }
    function test_rescheduled_slot_closes_old_details_data() {
        return [
            {tag:"start revised", startDelta:60000, duration:3600000},
            {tag:"duration revised", startDelta:0, duration:1800000}
        ]
    }
    function test_rescheduled_slot_closes_old_details(data) {
        guide.dayOffset = 1
        const start = guide.days[1].start
        guide.rows = [{index:0, label:"Channel 0", band:"GR", logo:""}]
        const identity = {endpoint:1, service:{network_id:4, service_id:42}, program:1, start:start, duration:3600000}
        const original = {watchKey:JSON.stringify(identity), name:"Same event", description:"Original", startAt:start, duration:3600000}
        guide.programsJson = JSON.stringify([{index:0, programs:[original]}])
        verify(waitForRendering(guide))
        mouseClick(findChild(guide, "guideCell"), 20, 30)
        const loader = findChild(guide, "scheduledDetailsLoader")
        tryCompare(loader.item, "opened", true)
        const revisedIdentity = Object.assign({}, identity, {start:start+data.startDelta, duration:data.duration})
        const revised = Object.assign({}, original, {watchKey:JSON.stringify(revisedIdentity), startAt:revisedIdentity.start, duration:data.duration})
        guide.programsJson = JSON.stringify([{index:0, programs:[revised]}])
        tryCompare(loader, "item", null)
        compare(guide.selectedProgram, null)
    }
    function test_detail_position_resize_and_animated_dismissal() {
        testCase.Window.window.width = 1440; testCase.Window.window.height = 900
        testCase.width = 1440; testCase.height = 900
        guide.width = 1440; guide.height = 880
        guide.rows = Array.from({length: 8}, (_, index) => ({index:index, label:"Channel " + index, band:"GR"}))
        verify(waitForRendering(guide))
        const timeline = findChild(guide, "guideTimeline").parent
        guide.selectedPosition = Qt.point(timeline.timeRulerWidth, 140)
        guide.selectedChannel = "101 NHK BS"
        guide.selectFixture({name:"World news",description:"Description",startAt:0,duration:1})
        const loader = findChild(guide, "scheduledDetailsLoader")
        const popup = loader.item
        tryCompare(popup, "opened", true)
        compare(popup.width, 560); compare(popup.height, 620)
        compare(popup.x - guide.selectedPosition.x - timeline.channelWidth, 24)
        compare(popup.y, 140)
        compare(popup.dim, false)
        compare(findChild(popup, "programChannel").text, "101 NHK BS")
        guide.selectedPosition = Qt.point(1100, 1000)
        compare(popup.x, 512)
        compare(popup.y, popup.parent.height - popup.height - 20)
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
        compare(guide.visibleChannels.count, 3)
        guide.visibilityJson = "[0,2,3]"
        const timeline = findChild(guide, "guideTimeline").parent
        compare(timeline.channels.count, 2)
        compare(timeline.channels.row(1).channelIndex, 2)
        guide.selectFixture({name:"Old details", startAt:0, duration:1})
        guide.visibilityJson = "[0,1,2,3]"
        compare(guide.selectedProgram, null)
        compare(timeline.channels.count, 3)
        guide.band = "BS"
        compare(timeline.channels.count, 1)
        compare(timeline.channels.row(0).channelIndex, 3)
        guide.visibilityJson = "[]"
        compare(timeline.channels.count, 0)
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
    function test_short_adjacent_programs_remain_individually_selectable() {
        guide.dayOffset = 1
        guide.rows = [{index:0, label:"Channel", band:"GR"}]
        const minuteMs = 60000
        const shortMinutes = 5
        const nextMinutes = 15
        const start = guide.days[1].start
        guide.programsJson = JSON.stringify([{index:0, programs:[
            {watchKey:"short", name:"Short news", startAt:start, duration:shortMinutes * minuteMs},
            {watchKey:"next", name:"Next program", startAt:start + shortMinutes * minuteMs, duration:nextMinutes * minuteMs}
        ]}])
        verify(waitForRendering(guide))
        const view = findChild(guide, "guideTimeline")
        const timeline = view.parent
        const loader = findChild(guide, "scheduledDetailsLoader")
        mouseClick(view, 20, timeline.channelHeaderHeight + shortMinutes * timeline.pixelsPerMinute / 2)
        tryCompare(loader.item, "opened", true)
        compare(guide.selectedProgram.watchKey, guide.keyFor("short"))
        keyClick(Qt.Key_Escape)
        tryCompare(loader, "item", null)
        mouseClick(view, 20, timeline.channelHeaderHeight + (shortMinutes + nextMinutes / 2) * timeline.pixelsPerMinute)
        tryCompare(loader.item, "opened", true)
        compare(guide.selectedProgram.watchKey, guide.keyFor("next"))
    }
    function test_toolbar_forwards_actions_and_fills_window_data() {
        return [
            {tag: "small component", width: 600, height: 460, language: "ja", bands: ["GR"], columns: 3},
            {tag: "minimum window", width: 900, height: 560, language: "ja", bands: ["GR", "BS", "CS"], columns: 5},
            {tag: "normal window", width: 1440, height: 900, language: "ja", bands: ["GR", "BS", "CS"], columns: 8},
            {tag: "all bands in English", width: 900, height: 560, language: "en", bands: ["GR", "BS", "CS", "SKY", "OTHER"], columns: 5}
        ]
    }
    function test_toolbar_forwards_actions_and_fills_window(data) {
        modesSpy.clear()
        closeSpy.clear()
        testCase.Window.window.width = data.width
        testCase.Window.window.height = data.height
        guide.width = data.width; guide.height = data.height
        guide.uiLanguage = data.language
        guide.targetWindow = guideWindow
        guide.rows = data.bands.map((band, index) => ({index:index, label:band, band:band}))
            .concat(Array.from({length: 8}, (_, index) => ({index:index + data.bands.length, label:"Channel " + index, band:"GR"})))
        const toolbar = findChild(guide, "guideToolbar")
        verify(waitForRendering(toolbar))
        const view = findChild(guide, "guideTimeline")
        const timeline = view.parent
        compare(toolbar.height, toolbar.twoRows ? 122 : 78)
        compare(timeline.y, toolbar.height)
        compare(timeline.y + timeline.height, guide.height)
        compare(view.x, 36)
        compare(view.width, guide.width - view.x)
        compare(findChild(guide, "guideChannelHeader").height, 40)
        compare(timeline.visibleChannelCount, data.columns)
        fuzzyCompare(timeline.channelWidth * data.columns, view.width, 0.01)
        const filters = findChild(guide, "guideFilters")
        const selector = findChild(guide, "guideDay")
        verify(selector.x + selector.width <= filters.width)
        verify(filters.mapToItem(guide, 0, 0).y + filters.height <= toolbar.height)
        verify(filters.x + filters.width <= guide.width)
        const help = findChild(toolbar, "guideHelpPopup")
        mouseClick(findChild(toolbar, "guideHelp"))
        tryCompare(help, "opened", true)
        keyClick(Qt.Key_Escape)
        tryCompare(help, "opened", false)
        const navigation = findChild(toolbar, "guideModeNavigation")
        compare(navigation.mapToItem(guide, 0, 0).y, 18)
        compare(guide.width - navigation.mapToItem(guide, navigation.width, 0).x, 18)
        compare(navigation.mode, Viewer.ModeNavigation.Guide)
        verify(findChild(navigation, "guideModeButton").active)
        for (const entry of [
            ["liveModeButton", Viewer.ModeNavigation.Live],
            ["recordingModeButton", Viewer.ModeNavigation.Recording],
            ["guideModeButton", Viewer.ModeNavigation.Guide],
            ["settingsModeButton", Viewer.ModeNavigation.Settings]
        ]) {
            modesSpy.clear()
            mouseClick(findChild(navigation, entry[0]))
            compare(modesSpy.count, 1)
            compare(modesSpy.signalArguments[0][0], entry[1])
        }
        mouseClick(findChild(toolbar, "closeGuide"))
        compare(closeSpy.count, 1)
    }
    function test_watch_key_is_unchanged_and_expired_program_cannot_request() {
        watchSpy.clear()
        const key = '{"endpoint":18446744073709551615}'
        const start = Date.now() - 1000
        guide.selectFixture({name:"Live",description:"Description",startAt:start,duration:60000,watchKey:key})
        const popup = findChild(guide, "scheduledDetailsLoader").item
        tryCompare(popup, "opened", true)
        const button = findChild(popup, "watchGuideProgram")
        verify(button.visible)
        mouseClick(button)
        compare(watchSpy.count, 1)
        compare(watchSpy.signalArguments[0][0], guide.keyFor(key))
        guide.watchError = "番組情報が更新されています"
        compare(findChild(popup, "watchGuideError").text, guide.watchError)
        guide.selectFixture({name:"長い番組名の表示確認 ".repeat(30),description:"Description",startAt:start,duration:60000,watchKey:key})
        guide.watchError = "番組情報が更新されています。番組表から選び直してください"
        verify(waitForRendering(guide))
        verify(button.y + button.height <= popup.availableHeight)
        popup.now = start + 60000
        compare(button.visible, false)
        popup.now = start - 1
        compare(button.visible, false)
    }
    function test_full_details_scroll_without_moving_grid_data() {
        return [
            {tag:"small", width:600, height:460},
            {tag:"wide", width:900, height:560}
        ]
    }
    function test_epg_sections_media_and_missing_fields_follow_refresh() {
        guide.dayOffset = 1
        const start = guide.selectedWindow.start
        const program = {
            watchKey:"metadata", startAt:start, duration:3600000, name:"Details", description:"Overview",
            genre:3, isFree:true,
            extended:[{heading:"番組内容", text:"長い番組内容\n".repeat(60)},
                {heading:"番組内容2", text:"More details"}, {heading:"出演者", text:"<b>Plain cast names</b>"},
                {heading:"スタッフ", text:"Staff"}, {heading:"", text:"Text without a heading"}],
            video:{type:"mpeg2", resolution:"1080i"},
            audios:[{componentTag:16, isMain:true, componentType:3, langs:["jpn"], samplingRate:48000},
                {componentTag:17, isMain:false, componentType:2, langs:["jpn", "eng"], samplingRate:32000}],
            series:{name:"Series title", episode:3, lastEpisode:12}
        }
        guide.rows = [{index:0, label:"Channel", band:"GR"}]
        guide.programsJson = JSON.stringify([{index:0, programs:[program]}])
        verify(waitForRendering(guide))
        mouseClick(findChild(guide, "guideCell"), 20, 30)
        const loader = findChild(guide, "scheduledDetailsLoader")
        const popup = loader.item
        tryCompare(popup, "opened", true)
        const metadata = findChild(popup, "programMetadata")
        const flick = findChild(popup, "programDetailsFlickable")
        const facts = findChild(popup, "programFacts")
        compare(facts.facts, [qsTranslate("Viewer", "Upcoming"), qsTranslate("Viewer", "%1 min").arg(60),
            qsTranslate("Viewer", "Drama"), qsTranslate("Viewer", "Free-to-air")])
        popup.now = start
        compare(facts.broadcastState, Viewer.ProgramFacts.OnAir)
        popup.now = start + program.duration
        compare(facts.broadcastState, Viewer.ProgramFacts.Finished)
        compare(metadata.sections.map(section => section.heading), program.extended.map(section => section.heading))
        const longText = findChild(metadata, "programExtendedText0")
        verify(longText.height > flick.height)
        compare(longText.truncated, false)
        compare(findChild(metadata, "programExtendedText2").text, "<b>Plain cast names</b>")
        compare(findChild(metadata, "programExtendedText2").textFormat, Text.PlainText)
        compare(findChild(metadata, "programExtendedText4").text, "Text without a heading")
        compare(metadata.broadcastFields.length, 4)
        verify(metadata.broadcastFields[0].text.indexOf("Series title") >= 0)
        verify(metadata.broadcastFields[0].text.indexOf("12") >= 0)
        compare(metadata.broadcastFields[1].text, "HD · 1080i / MPEG-2")
        compare(metadata.broadcastFields[2].heading, qsTranslate("Main", "Main audio"))
        verify(metadata.broadcastFields[2].text.indexOf("日本語") >= 0)
        verify(metadata.broadcastFields[2].text.indexOf("48 kHz") >= 0)
        compare(metadata.broadcastFields[3].heading, qsTranslate("Main", "Sub audio"))
        verify(metadata.broadcastFields[3].text.indexOf("日本語 / English") >= 0)
        const revised = Object.assign({}, program, {
            extended:[{heading:"更新された見出し", text:"Updated information"}],
            video:{type:"future-codec", resolution:"future-resolution"},
            audios:[{componentTag:17, isMain:false, componentType:254, langs:["unknown"], samplingRate:-1}],
            series:null, isFree:undefined
        })
        guide.programsJson = JSON.stringify([{index:0, programs:[revised]}])
        tryCompare(findChild(metadata, "programExtendedText0"), "text", "Updated information")
        compare(metadata.broadcastFields.length, 2)
        compare(metadata.broadcastFields[0].text, "future-resolution / future-codec")
        verify(metadata.broadcastFields[1].text.indexOf("0xfe") >= 0)
        verify(metadata.broadcastFields[1].text.indexOf("kHz") < 0)
        guide.programsJson = JSON.stringify([{index:0, programs:[{
            watchKey:program.watchKey, startAt:start, duration:program.duration, name:"Sparse EPG"
        }]}])
        tryCompare(metadata, "visible", false)
        compare(facts.facts.length, 2)
        compare(metadata.sections.length, 0)
        compare(metadata.broadcastFields.length, 0)
        verify(waitForRendering(popup.contentItem))
        verify(flick.atYBeginning)
        mouseClick(findChild(popup, "closeGuideProgramDetails"))
        tryCompare(loader, "item", null)
    }
    function test_full_details_scroll_without_moving_grid(data) {
        testCase.Window.window.width = data.width; testCase.Window.window.height = data.height
        testCase.width = data.width; testCase.height = data.height
        guide.width = data.width; guide.height = data.height
        const channel = "長い放送局名も最後まで確認できるチャンネル ".repeat(4)
        const program = {
            watchKey:"full-details", startAt:Date.now() - 60000, duration:3600000,
            name:"長い番組名のすべてを確認する特別番組 ".repeat(10),
            description:Array.from({length:16}, (_, index) => "第" + (index + 1) + "項目\n"
                + "出演者や番組の見どころを省略せずに表示します。 ".repeat(4)).join("\n\n") + "\n説明の最終行"
        }
        guide.rows = [{index:0, label:channel, band:"GR"}]
        guide.programsJson = JSON.stringify([{index:0, programs:[program]}])
        const view = findChild(guide, "guideTimeline")
        view.contentY = Math.max(0, (program.startAt - guide.selectedWindow.start) / 60000 * view.parent.pixelsPerMinute)
        verify(waitForRendering(guide))
        const cell = findChild(guide, "guideCell")
        mouseClick(cell, 20, 30)
        const loader = findChild(guide, "scheduledDetailsLoader")
        const popup = loader.item
        tryCompare(popup, "opened", true)
        const flick = findChild(popup, "programDetailsFlickable")
        const title = findChild(popup, "programTitle")
        const description = findChild(popup, "programDescription")
        const channelLabel = findChild(popup, "programChannel")
        compare(title.text, program.name)
        compare(title.truncated, false)
        verify(title.lineCount > 3)
        compare(channelLabel.text, channel)
        compare(channelLabel.truncated, false)
        verify(channelLabel.lineCount > 1)
        compare(description.text, program.description)
        compare(description.truncated, false)
        verify(description.height > flick.height)
        verify(flick.contentHeight > flick.height)
        compare(flick.contentY, 0)
        const gridPosition = Qt.point(view.contentX, view.contentY)
        const button = findChild(popup, "watchGuideProgram")
        const buttonPosition = button.mapToItem(popup.contentItem, 0, 0)
        keyClick(Qt.Key_Down)
        tryVerify(function() { return flick.contentY > 0 })
        const afterKey = flick.contentY
        mouseWheel(flick, flick.width / 2, flick.height / 2, 0, -120)
        tryVerify(function() { return flick.contentY > afterKey })
        mouseWheel(flick, flick.width / 2, flick.height / 2, 0, -120000)
        tryCompare(flick, "atYEnd", true)
        const descriptionBottom = description.mapToItem(flick, 0, description.height).y
        verify(descriptionBottom > 0 && descriptionBottom <= flick.height + 1)
        compare(Qt.point(view.contentX, view.contentY), gridPosition)
        compare(button.mapToItem(popup.contentItem, 0, 0), buttonPosition)
        verify(button.visible && buttonPosition.y + button.height <= popup.availableHeight)
        watchSpy.clear()
        mouseClick(button)
        compare(watchSpy.count, 1)
        compare(watchSpy.signalArguments[0][0], guide.keyFor(program.watchKey))
        // A failed watch request must reveal its error even with a long description.
        guide.watchError = "番組情報が更新されています"
        const error = findChild(popup, "watchGuideError")
        tryVerify(function() {
            const bottom = error.mapToItem(flick, 0, error.height).y
            return bottom > 0 && bottom <= flick.height + 1
        })
        keyClick(Qt.Key_Escape)
        tryCompare(loader, "item", null)
        guide.watchError = ""
        mouseClick(cell, 20, 30)
        tryCompare(loader.item, "opened", true)
        compare(findChild(loader.item, "programDetailsFlickable").contentY, 0)
    }
    function test_details_allow_toolbar_and_outside_click_dismisses() {
        modesSpy.clear()
        guide.selectFixture({name:"Program",description:"Description",startAt:0,duration:1})
        const loader = findChild(guide, "scheduledDetailsLoader")
        tryCompare(loader.item, "opened", true)
        mouseClick(findChild(guide, "settingsModeButton"))
        compare(modesSpy.count, 1)
        tryCompare(loader, "item", null)
        guide.selectFixture({name:"Program",description:"Description",startAt:0,duration:1})
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
        tryCompare(view, "contentX", view.parent.channelWidth)
        compare(view.contentY, 0)
        mouseWheel(wheel, 20, 130, 0, -120, Qt.NoButton, Qt.NoModifier)
        tryVerify(function() { return view.contentY > 0 })
        compare(view.contentX, view.parent.channelWidth)
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
        guide.rows = [{index:0, label:"Channel", band:"GR"}]
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
        compare(findChild(guide, "guideColumn29"), null)
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
        compare(guide.selectedPosition.x, list.x + list.parent.dividerWidth)
        compare(findChild(loader.item, "programDescription").text, "Full description 0")
        list.contentX = list.contentWidth - list.width
        verify(waitForRendering(guide))
        tryVerify(function() { return findChild(guide, "guideColumn0") === null })
        verify(findChild(guide, "guideColumn29").item !== null)
        compare(findChild(loader.item, "programTitle").text, "Program 0")
        guide.dayOffset = 2
        compare(loader.item, null)
        compare(guide.selectedProgram, null)
    }

    function test_virtual_columns_stay_aligned_after_scroll_and_filter_changes() {
        guide.dayOffset = 1
        guide.rows = Array.from({length:40}, (_,i) =>
            ({index:i, label:"Channel "+i, band:i<20 ? "GR" : "BS"}))
        guide.programsJson = JSON.stringify(guide.rows.map(row => ({index:row.index,
            programs:[{name:"Program "+row.index, startAt:guide.selectedWindow.start, duration:3600000}]})))
        const view = findChild(guide, "guideTimeline")
        const timeline = view.parent
        verify(waitForRendering(guide))
        view.contentX = view.contentWidth - view.width
        verify(waitForRendering(guide))
        const last = findChild(guide, "guideColumn19")
        verify(last !== null)
        fuzzyCompare(last.mapToItem(view,0,0).x + last.width, view.width, 1)
        mouseClick(view, view.width-20, timeline.channelHeaderHeight+20)
        compare(guide.selectedChannel, "Channel 19")
        compare(guide.selectedProgram.name, "Program 19")
        keyClick(Qt.Key_Escape)
        const details = findChild(guide, "scheduledDetailsLoader")
        tryCompare(details, "item", null)
        // Model insertions/removals can move ListView's origin. Both filters
        // must still put the first surviving channel at the viewport edge.
        for (const band of ["BS", "GR"]) {
            guide.band = band
            tryCompare(view, "contentX", 0)
            guide.visibilityJson = JSON.stringify(band === "BS" ? [21,23,25,27] : [1,3,5,7])
            verify(waitForRendering(guide))
            const first = findChild(guide, "guideColumn0")
            verify(first !== null)
            fuzzyCompare(first.mapToItem(view,0,0).x, 0, 1)
            mouseClick(view, 20, timeline.channelHeaderHeight+20)
            compare(guide.selectedChannel, "Channel " + (band === "BS" ? 21 : 1))
            compare(guide.selectedProgram.name, "Program " + (band === "BS" ? 21 : 1))
            keyClick(Qt.Key_Escape)
            tryCompare(details, "item", null)
            guide.visibilityJson = "null"
        }
    }

    function test_vertical_window_bounds_cells_and_keeps_offscreen_selection() {
        guide.dayOffset = 1
        guide.rows = [{index:0, label:"Channel", band:"GR"}]
        const start = guide.selectedWindow.start
        const duration = 15 * 60000
        const programs = Array.from({length:96}, (_,i) => ({watchKey:"slot"+i,
            name:"Program "+i, startAt:start+i*duration, duration:duration}))
        guide.programsJson = JSON.stringify([{index:0, programs:programs}])
        const view = findChild(guide, "guideTimeline")
        const timeline = view.parent
        verify(waitForRendering(guide))
        const slots = []
        function collect(item) {
            if (item.objectName === "guideCellLoader") slots.push(item)
            for (const child of item.children || []) collect(child)
        }
        collect(guide)
        compare(slots.length, programs.length)
        compare(guide.guideModel.count, programs.length)
        const pixelHeight = duration / 60000 * timeline.pixelsPerMinute
        const limit = Math.ceil((view.height + 2*timeline.verticalPreload) / pixelHeight) + 2
        verify(slots.filter(slot => slot.item !== null).length <= limit)
        const first = slots.find(slot => slot.watchKey === guide.keyFor("slot0"))
        const noon = slots.find(slot => slot.watchKey === guide.keyFor("slot48"))
        verify(first.item !== null)
        compare(noon.item, null)
        const aheadIndex = Math.floor((view.height + timeline.verticalPreload / 2 - timeline.channelHeaderHeight) / pixelHeight)
        const ahead = slots.find(slot => slot.watchKey === guide.keyFor("slot"+aheadIndex))
        verify(ahead.asynchronous)
        verify(ahead.active)
        tryCompare(ahead, "status", Loader.Ready)
        const preparedCell = ahead.item
        view.contentY = aheadIndex * pixelHeight
        verify(waitForRendering(guide))
        compare(ahead.asynchronous, false)
        compare(ahead.item, preparedCell)
        // Wheel steps and scrollbar jumps must have complete visible cells as
        // soon as the position changes, even with preloads still pending.
        for (const offset of [0,144,288,2000,2144]) {
            view.contentY = offset
            for (const slot of slots) {
                if (slot.y + slot.height > view.contentY + timeline.channelHeaderHeight
                        && slot.y < view.contentY + view.height) {
                    compare(slot.status, Loader.Ready)
                    verify(slot.item !== null)
                }
            }
        }
        view.contentY = 48 * pixelHeight
        verify(waitForRendering(guide))
        compare(first.item, null)
        verify(noon.item !== null)
        verify(slots.filter(slot => slot.item !== null).length <= limit)
        mouseClick(noon.item, 20, 15)
        const details = findChild(guide, "scheduledDetailsLoader")
        tryCompare(details.item, "opened", true)
        compare(guide.selectedProgram.watchKey, guide.keyFor("slot48"))
        view.contentY = 0
        verify(waitForRendering(guide))
        compare(noon.item, null)
        compare(findChild(details.item, "programTitle").text, "Program 48")
        keyClick(Qt.Key_Escape)
        tryCompare(details, "item", null)
        timeline.chooseProgram(start + 90 * duration)
        timeline.revealCursor()
        verify(waitForRendering(guide))
        keyClick(Qt.Key_Return)
        tryCompare(details.item, "opened", true)
        compare(guide.selectedProgram.watchKey, guide.keyFor("slot90"))
    }

    function test_pending_preloads_survive_snapshot_and_band_changes() {
        guide.dayOffset = 1
        guide.rows = [{index:0, label:"Channel", band:"GR"}]
        const start = guide.selectedWindow.start
        function snapshot(prefix) {
            return JSON.stringify([{index:0, programs:Array.from({length:96}, (_,i) =>
                ({watchKey:"slot"+i, name:prefix+i, startAt:start+i*15*60000, duration:15*60000}))}])
        }
        guide.programsJson = snapshot("Before ")
        verify(waitForRendering(guide))
        const view = findChild(guide, "guideTimeline")
        view.contentY = 8*60*view.parent.pixelsPerMinute
        guide.programsJson = snapshot("After ")
        guide.band = "BS"
        guide.band = "GR"
        verify(waitForRendering(guide))
        view.contentY = 12*60*view.parent.pixelsPerMinute
        verify(waitForRendering(guide))
        mouseClick(view, 20, view.parent.channelHeaderHeight+15)
        const details = findChild(guide, "scheduledDetailsLoader")
        tryCompare(details.item, "opened", true)
        compare(guide.selectedProgram.name, "After 48")
        compare(guide.selectedProgram.watchKey, guide.keyFor("slot48"))
    }

    function test_conflict_segments_candidates_and_channel_action() {
        guide.dayOffset = 1
        guide.rows = [{index:0, label:"Conflict channel", band:"GR"}]
        const start = guide.selectedWindow.start
        const minute = 60000
        const a = {watchKey:"a", name:"Long program", startAt:start, duration:60*minute}
        const b = {watchKey:"b", name:"Short program", startAt:start+15*minute, duration:15*minute}
        const c = {watchKey:"c", name:"Later program", startAt:start+45*minute, duration:45*minute}
        guide.programsJson = JSON.stringify([{index:0, programs:[a,b,c]}])
        compare(guide.guideModel.count, 5)
        for (let i=1; i<guide.guideModel.count; ++i)
            verify(guide.guideModel.row(i-1).end <= guide.guideModel.row(i).begin)
        const conflict = guide.guideModel.row(1)
        compare(conflict.scheduleState, Viewer.GuideModel.Conflict)
        compare(conflict.candidateCount, 2)
        const timeline = findChild(guide,"guideTimeline").parent
        timeline.chooseProgram(start+20*minute)
        compare(timeline.cursorKey, conflict.watchKey)
        guide.selectedProgram = conflict
        const popup = findChild(guide,"scheduledDetailsLoader").item
        tryCompare(popup,"opened",true)
        popup.now = start+20*minute
        const button = findChild(popup,"watchGuideProgram")
        verify(button.visible)
        compare(button.text,qsTranslate("Viewer","Watch this channel"))
        compare(popup.candidates.count,2)
        const selector = findChild(popup,"guideCandidateSelector")
        verify(selector.visible)
        mouseClick(selector)
        keyClick(Qt.Key_Down)
        keyClick(Qt.Key_Return)
        tryCompare(popup,"candidateIndex",1)
        compare(findChild(popup,"programTitle").text,"Short program")
        watchSpy.clear()
        tryVerify(function() { return !selector.popup.visible })
        popup.now = start+20*minute
        mouseClick(button)
        compare(watchSpy.count, 1)
        compare(watchSpy.signalArguments[0][0],conflict.watchKey)
        guide.programsJson = JSON.stringify([{index:0,programs:[a,c]}])
        tryCompare(findChild(guide,"scheduledDetailsLoader"),"item",null)
    }
    function test_unknown_end_uses_display_bound_without_claiming_an_end() {
        guide.dayOffset = 1
        guide.rows = [{index:0,label:"Unknown end",band:"GR"}]
        const start=guide.selectedWindow.start
        guide.programsJson=JSON.stringify([{index:0,programs:[
            {watchKey:"unknown",name:"Unknown end",startAt:start,duration:1},
            {watchKey:"next",name:"Next",startAt:start+3600000,duration:3600000}
        ]}])
        const row=guide.guideModel.row(0)
        compare(row.scheduleState,Viewer.GuideModel.UnknownEnd)
        compare(row.end-row.begin,3600000)
        guide.selectedProgram=row
        const popup=findChild(guide,"scheduledDetailsLoader").item
        tryCompare(popup,"opened",true)
        verify(findChild(popup,"programDateTime").text.includes(qsTranslate("Viewer","End time unknown")))
        popup.now=start+1000
        compare(popup.watchAction,Viewer.GuideModel.WatchChannel)
        popup.now=start+3600000
        compare(popup.watchAction,Viewer.GuideModel.Unavailable)
    }
}
