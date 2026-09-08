import QtQuick
import QtTest
import MinimalViewer
import "../../rust/qml" as Viewer

TestCase {
    TestOutlineProvider { id: subtitleOutlines }
    id: testCase
    name: "SubtitleMemory"
    when: windowShown
    visible: true
    width: 1440; height: 810
    Viewer.SubtitleOverlay {
        id: overlay
        anchors.fill: parent
        captionJson: ""
        outlineProvider: subtitleOutlines
        fontSource: Qt.resolvedUrl("../../assets/fonts/rounded-mplus-1m-arib.ttf")
    }
    function initTestCase() {
        testCase.Window.window.width = 1440
        testCase.Window.window.height = 810
    }
    function cue(page) {
        const cells = []
        for (let i = 0; i < 32; ++i) {
            cells.push({text:String.fromCharCode(0x4e00 + page * 32 + i),
                x:24+(i%16)*56, y:350+Math.floor(i/16)*60, width:52, height:56,
                glyphWidth:36, glyphHeight:36, foreground:"#ffffffff", background:"#80000000",
                stroke:"#ff000000", bold:false, italic:false, underline:false, stroked:subtitleOutlines.benchmark_stroke, ruby:false})
        }
        return JSON.stringify({text:"",planeWidth:960,planeHeight:540,cells:cells})
    }
    function mark(phase) {
        // The caller already waited for the frame caused by its last change.
        wait(500)
        console.log("MEMORY_PHASE " + phase)
        wait(100)
    }
    function test_repeated_glyph_set() {
        failOnWarning(/.*/)
        tryCompare(findChild(overlay,"subtitleFont"),"status",FontLoader.Ready)
        verify(waitForRendering(overlay))
        mark("loaded")
        for (let round=1; round<=5; ++round) {
            for (let page=0; page<8; ++page) {
                // Passes 4 and 5 introduce and then repeat a different 256-glyph set.
                overlay.captionJson=cue((round >= 4 ? 8 : 0) + page)
                verify(waitForRendering(overlay))
                wait(40)
            }
            mark("round"+round)
            overlay.captionJson=""
            verify(waitForRendering(overlay))
            mark("clear"+round)
        }
        if (subtitleOutlines.benchmark_stroke)
            verify(subtitleOutlines.calls > 0)
        else
            compare(subtitleOutlines.calls, 0)
        console.log("OUTLINE_CALLS " + subtitleOutlines.calls)
    }
}
