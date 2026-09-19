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
    function test_normal_choices_and_evaluation_only_option() {
        const controls = createTemporaryObject(component, this);
        const motion = findChild(controls, "commentMotion");
        const placement = findChild(controls, "commentPlacement");
        compare(placement.options.length, 2);
        mouseClick(findChild(controls, "motion-pop"));
        compare(controls.displayMode, "pop");
        mouseClick(findChild(controls, "placement-random"));
        compare(controls.placementMode, "random");
        controls.evaluationCollision = true;
        compare(placement.options.length, 2);
        mouseClick(findChild(controls, "motion-scroll"));
        compare(controls.displayMode, "scroll");
        compare(placement.options.length, 3);
        mouseClick(findChild(controls, "placement-collision"));
        compare(controls.placementMode, "collision");
        mouseClick(findChild(controls, "motion-pop"));
        compare(controls.displayMode, "pop");
        compare(controls.placementMode, "sequential");
        compare(placement.options.length, 2);
    }
}
