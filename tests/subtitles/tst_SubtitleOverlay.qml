import QtQuick
import QtTest
import "../../rust/qml" as Viewer

TestCase {
    id: testCase
    name: "SubtitleRendering"
    when: windowShown
    visible: true
    width: 960
    height: 540

    Component {
        id: component
        Loader {
            width: 960
            height: 540
            property string caption: ""
            sourceComponent: Viewer.SubtitleOverlay {
                captionJson: parent.caption
                outlineProvider: subtitleOutlines
                fontSource: Qt.resolvedUrl("../../assets/fonts/rounded-mplus-1m-arib.ttf")
            }
        }
    }
    property var holder
    function initTestCase() { failOnWarning(/.*/) }
    function init() {
        holder = createTemporaryObject(component, testCase)
        verify(holder !== null)
        tryCompare(findChild(holder, "subtitleFont"), "status", FontLoader.Ready)
    }
    function screen(text, stroked, cellsOnly) {
        return JSON.stringify({ text: text, planeWidth: 960, planeHeight: 540, cells: cellsOnly ? [] : [{
            text: text, x: 100, y: 200, width: 80, height: 90,
            glyphWidth: 60, glyphHeight: 60,
            foreground: "#ffffffff", background: "#00000000", stroke: "#ffff0000",
            bold: false, italic: false, underline: false, stroked: stroked, ruby: false
        }] })
    }
    function test_baseline_font_and_outline_pixels_data() {
        return [{tag: "small kana", text: "ぁ"}, {tag: "midline bar", text: "ー"},
                {tag: "descender", text: "g"}, {tag: "kanji", text: "字"}]
    }
    function test_baseline_font_and_outline_pixels(data) {
        holder.caption = screen(data.text, true, false)
        const glyph = findChild(holder, "subtitleGlyph")
        verify(glyph !== null)
        compare(glyph.font.family, findChild(holder, "subtitleFont").name)
        compare(glyph.font.pixelSize, 60)
        const outline = findChild(glyph, "outlineLoader")
        compare(outline.y, glyph.baselineOffset)
        compare(outline.x, glyph.leftPadding)
        verify(outline.item !== null)
        verify(waitForRendering(holder))
        const picture = grabImage(holder)
        let red = 0
        let white = 0
        // A real CurveRenderer outline must produce red pixels around white text.
        const sx = picture.width / holder.width
        const sy = picture.height / holder.height
        for (let y = Math.floor(192 * sy); y < Math.ceil(298 * sy); ++y) {
            for (let x = Math.floor(92 * sx); x < Math.ceil(188 * sx); ++x) {
                if (picture.alpha(x, y) < 100) continue
                if (picture.red(x, y) > 200 && picture.green(x, y) < 60) red++
                if (picture.red(x, y) > 200 && picture.green(x, y) > 200) white++
            }
        }
        verify(red > 20, "no visible red outline")
        verify(white > 20, "no visible glyph fill")
    }
    function test_no_outline_work_without_stroke_or_without_new_cue() {
        const start = subtitleOutlines.calls
        holder.caption = screen("字幕", false, false)
        compare(findChild(holder, "outlineLoader").item, null)
        compare(subtitleOutlines.calls, start)
        holder.caption = screen("字幕", true, false)
        verify(subtitleOutlines.calls > start)
        const built = subtitleOutlines.calls
        wait(100)
        compare(subtitleOutlines.calls, built)
        holder.caption = screen("字幕", false, false)
        compare(findChild(holder, "outlineLoader").item, null)
    }
    function test_resize_scales_cell_and_outline() {
        holder.caption = screen("ー", true, false)
        holder.width = 480
        holder.height = 270
        const glyph = findChild(holder, "subtitleGlyph")
        compare(glyph.parent.x, 50)
        compare(glyph.parent.y, 100)
        compare(glyph.parent.width, 40)
        compare(glyph.font.pixelSize, 30)
        fuzzyCompare(glyph.outlineRadius, 1.8, 0.001)
        compare(findChild(glyph, "outlineLoader").y, glyph.baselineOffset)
    }
    function test_clear_and_disable_release_delegates() {
        holder.caption = screen("字", true, false)
        compare(findChild(holder, "captionCells").count, 1)
        holder.caption = ""
        compare(findChild(holder, "captionCells").count, 0)
        compare(findChild(holder, "plainCaption").text, "")
        holder.caption = screen("文字だけ", false, true)
        compare(findChild(holder, "plainCaption").text, "文字だけ")
        compare(findChild(holder, "plainCaption").visible, true)
        holder.active = false
        compare(holder.item, null)
        const calls = subtitleOutlines.calls
        wait(50)
        compare(subtitleOutlines.calls, calls)
    }
}
