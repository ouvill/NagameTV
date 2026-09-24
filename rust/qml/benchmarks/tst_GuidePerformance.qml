import QtQuick
import QtTest
import MinimalViewer as Viewer

// Optional rendering benchmark, outside the normal QML test suite.
// See docs/guide-performance.md for the validated display and build commands.
TestCase {
    id: testCase
    name: "GuidePerformance"
    when: windowShown
    visible: true
    width: 1280; height: 720
    Viewer.ChannelModel { id: channelFixture }
    Viewer.GuideModel { id: scheduleFixture }
    Viewer.GuideModel { id: detached }
    Window { id: guideWindow }
    Component {
        id: component
        Viewer.ProgramGuide {
            width: 1280; height: 720
            guideModel: scheduleFixture; channels: channelFixture
            targetWindow: guideWindow
            status: "Ready"; channel: "Benchmark"; selected: 0; dayOffset: 1
        }
    }
    function cells(item) {
        let count = item.objectName === "guideCell" ? 1 : 0
        for (const child of item.children || []) count += cells(child)
        return count
    }
    function summary(samples) {
        const sorted=samples.slice().sort((a,b)=>a-b)
        return {samples:samples, median:sorted[Math.floor(sorted.length/2)], max:sorted[sorted.length-1]}
    }
    function verifyViewport(guide, firstChannel) {
        const view=findChild(guide,"guideTimeline")
        const timeline=view.parent
        for (let i=0;i<timeline.visibleChannelCount;i++) {
            const column=findChild(guide,"guideColumn"+i)
            verify(column !== null)
            verify(column.item !== null)
            compare(column.channelIndex,firstChannel+i)
            fuzzyCompare(column.mapToItem(view,0,0).x,i*timeline.channelWidth,1)
            verify(cells(column)>0)
        }
    }
    function initTestCase() { failOnWarning(/.*/) }
    function test_timings() {
        testCase.Window.window.width=1280
        testCase.Window.window.height=720
        const day=86400000
        const date=new Date(); date.setHours(0,0,0,0)
        const base=date.getTime()
        const channelCount=54, programsPerDay=48, repetitions=7
        verify(channelFixture.load_test(JSON.stringify(Array.from({length:channelCount},(_,i)=>({index:i,label:"Channel "+i,band:i<27?"GR":"BS",logo:""})))))
        const inputs=[1,2].map(offset=>JSON.stringify(Array.from({length:channelCount},(_,channel)=>({index:channel,programs:Array.from({length:programsPerDay},(_,i)=>({watchKey:channel+"/"+offset+"/"+i,startAt:base+day*offset+i*day/programsPerDay,duration:day/programsPerDay,name:"番組タイトル "+i+"　放送予定の説明",description:"番組の概要と出演者などの説明文です。".repeat(20),genre:i%12}))}))))
        const preparation=[],open=[],openFrame=[],change=[],changeFrame=[],band=[],bandFrame=[]
        let delegateCount=0
        for (let i=0;i<repetitions;i++) {
            let before=Date.now()
            verify(detached.load_test(inputs[0]))
            preparation.push(Date.now()-before)
            verify(scheduleFixture.load_test(inputs[0]))
            before=Date.now()
            const guide=component.createObject(testCase)
            verify(guide !== null)
            open.push(Date.now()-before)
            verify(waitForRendering(guide))
            openFrame.push(Date.now()-before)
            verifyViewport(guide,0)
            delegateCount=cells(guide)
            verify(delegateCount > 0)
            before=Date.now()
            guide.dayOffset=2
            verify(scheduleFixture.load_test(inputs[1]))
            change.push(Date.now()-before)
            verify(waitForRendering(guide))
            changeFrame.push(Date.now()-before)
            verifyViewport(guide,0)
            before=Date.now()
            guide.band="BS"
            band.push(Date.now()-before)
            verify(waitForRendering(guide))
            bandFrame.push(Date.now()-before)
            verifyViewport(guide,27)
            verify(cells(guide)>0)
            guide.destroy()
            wait(30)
            gc()
        }
        console.log("GUIDE_BENCH "+JSON.stringify({channels:channelCount,programsPerDay:programsPerDay,delegates:delegateCount,prepareOnlyMs:summary(preparation),openSyncMs:summary(open),openFrameMs:summary(openFrame),changeWithFixtureMs:summary(change),changeFrameWithFixtureMs:summary(changeFrame),bandSyncMs:summary(band),bandFrameMs:summary(bandFrame)}))
    }
}
