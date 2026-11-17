import QtQuick
import QtTest
import MinimalViewer as Viewer

TestCase {
    name: "VideoCommentBounds"
    Component { id: bounds; Viewer.VideoCommentBounds { viewportWidth: 960; viewportHeight: 540; aspectRatio: 16 / 9 } }
    function test_actual_picture_only_data() {
        return [
            {tag: "wide", aspect: 16 / 9, w: 960, h: 540, x: 0, y: 0},
            {tag: "pillarbox", aspect: 4 / 3, w: 720, h: 540, x: 120, y: 0},
            {tag: "cinema", aspect: 2.4, w: 960, h: 400, x: 0, y: 70},
            {tag: "portrait", aspect: 9 / 16, w: 303, h: 540, x: 328, y: 0},
            {tag: "unknown", aspect: 0, w: 0, h: 0, x: 480, y: 270}
        ];
    }
    function test_actual_picture_only(data) {
        const item = createTemporaryObject(bounds, this, {aspectRatio: data.aspect});
        compare(item.width, data.w); compare(item.height, data.h);
        compare(item.x, data.x); compare(item.y, data.y); verify(item.clip);
        item.evaluationWide = true;
        compare(item.width, 960); compare(item.height, 540);
        item.evaluationWide = false;
        compare(item.width, data.w);
    }
}
