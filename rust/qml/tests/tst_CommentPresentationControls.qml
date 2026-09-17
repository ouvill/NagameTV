import QtQuick
import QtTest
import ".." as Viewer

TestCase {
    id: testCase
    name: "CommentPresentationControls"
    when: windowShown
    visible: true
    width: 400
    height: 400
    Component {
        id: component
        Viewer.CommentPresentation {
            width: 320
            onSelected: function(display, placement) { displayMode = display; placementMode = placement; }
        }
    }
    function initTestCase() { failOnWarning(/.*/); }
    function choose(combo, key) {
        mouseClick(combo);
        tryCompare(combo.popup, "opened", true);
        keyClick(key);
        keyClick(Qt.Key_Return);
        tryCompare(combo.popup, "visible", false);
        waitForRendering(combo.parent);
    }
    function test_normal_choices_and_evaluation_only_option() {
        const controls = createTemporaryObject(component, this);
        const motion = findChild(controls, "commentMotion");
        const placement = findChild(controls, "commentPlacement");
        compare(placement.count, 2);
        choose(motion, Qt.Key_End);
        compare(controls.displayMode, "pop");
        choose(placement, Qt.Key_End);
        compare(controls.placementMode, "random");
        controls.evaluationCollision = true;
        compare(placement.count, 2);
        choose(motion, Qt.Key_Home);
        compare(controls.displayMode, "scroll");
        compare(placement.count, 3);
        choose(placement, Qt.Key_End);
        compare(controls.placementMode, "collision");
        choose(motion, Qt.Key_End);
        compare(controls.displayMode, "pop");
        compare(controls.placementMode, "sequential");
        compare(placement.count, 2);
    }
}
