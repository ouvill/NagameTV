from pathlib import Path
import json
import sys
base = Path('benchmark/browser-design').resolve()
base.mkdir(parents=True, exist_ok=True)
reference_path = Path(sys.argv[1] if len(sys.argv) > 1 else '/project/qml/Main.qml').resolve()
source = reference_path.read_text()
def block(marker):
    start = source.index(marker)
    opening = source.index('{', start)
    depth = 0
    for i in range(opening, len(source)):
        depth += (source[i] == '{') - (source[i] == '}')
        if depth == 0:
            return source[start:i+1]
    raise ValueError(marker)
parts = [block('component RoundAction:'), block('component BroadcastTabs:'), block('Rectangle {\n        id: channelPicker')]
reference = '''import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
Control {
 id: root
 width: 1440; height: 304
 font.family: "Noto Sans CJK JP"
 property bool channelsOpen: true
 property string channelPickerType: "GR"
 readonly property color ink: "#f4f5f3"
 readonly property color muted: "#b6bab6"
 readonly property color accent: "#9caf9f"
 readonly property color raised: "#1c1f1c"
 property QtObject player: QtObject {
  property string uiLanguage: "ja"
  property var services: ["01   総合テレビ", "02   教育テレビ", "03   地域テレビ", "101   BSテレビ"]
  property var channelTypes: ["GR", "GR", "GR", "BS"]
  property var channelLogoUrls: ["", "", "", ""]
  property string channelName: services[0]
  property var programTitles: ["街の風景と暮らしを訪ねて", "科学の時間", "ニュース", "映画"]
  function selectChannel(index) {}
 }
 function uiIcon(name) { return "file:///project/assets/icons/" + name + ".svg" }
 function availableChannelTypes() { return [["GR", "地デジ"], ["BS", "BS"]] }
 function jikkyoForce(index) { return "" }
 function programTime(index) { return "20:00 – 20:54" }
 function programProgressAt(index) { return 0.5 }
 function scrollOneStep() {}
'''+ '\n'.join(parts) +'\n}'
reference = reference.replace('qsTr("Channels")','"チャンネル"').replace('qsTr("Channel logo")','"局ロゴ"')
reference = reference.replace('file:///project/assets/icons/', (reference_path.parent.parent / 'assets/icons').as_uri() + '/')
(base / 'Reference.qml').write_text(reference)
fixture = '''import QtQuick
import QtQuick.Controls
import QtTest
import "../../rust/qml" as Viewer
TestCase {
 id: testCase
 name: "BrowserDesign"
 when: windowShown
 visible: true
 width: 1440; height: 304
 Rectangle { id: backdrop; anchors.fill: parent; color: "#39444c"
  Reference { id: reference; anchors.fill: parent }
  Viewer.ChannelBrowser {
   id: candidate; anchors.fill: parent; visible: false
   rows: [ {index:0,label:"01   総合テレビ",band:"GR",logo:""}, {index:1,label:"02   教育テレビ",band:"GR",logo:""}, {index:2,label:"03   地域テレビ",band:"GR",logo:""}, {index:3,label:"101   BSテレビ",band:"BS",logo:""} ]
   selected: 0
   property real start: new Date(2026, 8, 7, 20, 0).getTime()
   now: start + 27 * 60000
   programsJson: JSON.stringify(["街の風景と暮らしを訪ねて", "科学の時間", "ニュース", "映画"].map(name => ({name: name, startAt: start, duration: 54*60000})))
  }
 }
 function test_capture_same_fixture() {
  failOnWarning(/.*/);
  wait(350);
  const mainImage = grabImage(backdrop);
  verify(mainImage.red(20, 20) < 100, "Reference must be rendered, not a blank capture");
  mainImage.save(OUTPUT + "/main.png");
  reference.visible = false;
  candidate.visible = true;
  wait(350);
  const candidateImage = grabImage(backdrop);
  verify(candidateImage.red(20, 20) < 100, "Candidate must be rendered, not a blank capture");
  candidateImage.save(OUTPUT + "/candidate.png");
 }
}
'''.replace('OUTPUT', json.dumps(str(base)))
(base / 'tst_Compare.qml').write_text(fixture)
