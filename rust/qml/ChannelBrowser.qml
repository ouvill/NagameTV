pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Pane {
    id: root
    required property var rows
    required property int selected
    property string programsJson: "[]"
    property real now: 0
    readonly property var programs: JSON.parse(programsJson)
    property string band: "GR"
    readonly property var filteredRows: rows.filter(row => row.band === band)
    signal selectRequested(int index)
    signal closeRequested
    function focusBrowser() {
        list.forceActiveFocus();
    }
    function resetCursor() {
        // Required inputs may arrive before the derived binding and child view
        // are initialized. Component.onCompleted performs the initial selection.
        if (!list || !filteredRows)
            return;
        const selectedRow = filteredRows.findIndex(row => row.index === selected);
        list.currentIndex = selectedRow >= 0 ? selectedRow : (filteredRows.length ? 0 : -1);
    }
    function selectCurrent() {
        const row = filteredRows[list.currentIndex];
        if (row)
            selectRequested(row.index);
    }
    onFilteredRowsChanged: resetCursor()
    onSelectedChanged: resetCursor()
    Component.onCompleted: {
        const current = rows.find(row => row.index === selected) || rows[0];
        if (current)
            band = current.band;
        resetCursor();
    }
    background: Rectangle {
        color: "#ed151515"
    }
    contentItem: ColumnLayout {
        RowLayout {
            Layout.fillWidth: true
            Label {
                text: "チャンネル"
                color: "white"
                font.pixelSize: 20
            }
            ComboBox {
                objectName: "browserBand"
                model: ["GR", "BS", "CS", "SKY", "OTHER"]
                currentIndex: model.indexOf(root.band)
                onActivated: root.band = currentText
            }
            Item {
                Layout.fillWidth: true
            }
            Button {
                text: "閉じる"
                onClicked: root.closeRequested()
            }
        }
        ListView {
            id: list
            objectName: "browserList"
            Layout.fillWidth: true
            Layout.fillHeight: true
            orientation: ListView.Horizontal
            spacing: 8
            clip: true
            cacheBuffer: 0
            model: root.filteredRows
            keyNavigationEnabled: true
            Keys.onReturnPressed: root.selectCurrent()
            Keys.onEnterPressed: root.selectCurrent()
            ScrollBar.horizontal: ScrollBar {}
            delegate: ItemDelegate {
                id: card
                required property var modelData
                required property int index
                width: 240
                height: list.height - 14
                highlighted: modelData.index === root.selected
                onClicked: root.selectRequested(modelData.index)
                background: Rectangle {
                    radius: 8
                    color: card.highlighted ? "#284a40" : "#252a2d"
                }
                contentItem: ColumnLayout {
                    ChannelLogo {
                        logoUrl: card.modelData.logo || ""
                    }
                    Label {
                        Layout.fillWidth: true
                        text: card.modelData.label
                        color: "#eeeeee"
                        textFormat: Text.PlainText
                        wrapMode: Text.Wrap
                        maximumLineCount: 2
                        elide: Text.ElideRight
                    }
                    Item {
                        Layout.fillHeight: true
                    }
                    ChannelProgram {
                        Layout.fillWidth: true
                        program: root.programs[card.modelData.index] || null
                        now: root.now
                    }
                }
                Rectangle {
                    anchors.fill: parent
                    color: "transparent"
                    border.width: 2
                    border.color: "#8ac7ae"
                    visible: list.activeFocus && list.currentIndex === card.index
                }
            }
            Label {
                anchors.centerIn: parent
                visible: list.count === 0
                text: "該当するチャンネルなし"
                color: "#cccccc"
            }
        }
    }
}
