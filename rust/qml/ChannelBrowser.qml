pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Pane {
    id: root
    required property var rows
    required property int selected
    property string visibilityJson: "[]"
    readonly property var visibleIndices: new Set(JSON.parse(visibilityJson))
    property string programsJson: "[]"
    property real now: 0
    readonly property var programs: JSON.parse(programsJson)
    property string band: "GR"
    readonly property var filteredRows: rows.filter(row => row.band === band && (!visibleIndices.size || visibleIndices.has(row.index)))
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
    implicitHeight: 304
    leftPadding: 24
    rightPadding: 24
    topPadding: 20
    bottomPadding: 38
    font.family: "Noto Sans CJK JP"
    background: Rectangle {
        gradient: Gradient {
            GradientStop {
                position: 0
                color: "#06000000"
            }
            GradientStop {
                position: 0.35
                color: "#52000000"
            }
            GradientStop {
                position: 1
                color: "#d6000000"
            }
        }
    }
    contentItem: ColumnLayout {
        spacing: 14
        RowLayout {
            Layout.fillWidth: true
            spacing: 14
            BrowserCollapseButton {
                onClicked: root.closeRequested()
            }
            Label {
                text: qsTranslate("Main", "Channels")
                color: "#f4f5f3"
                font.pixelSize: 22
                font.bold: true
            }
            Item {
                Layout.preferredWidth: 24
            }
            BroadcastTabs {
                objectName: "browserBand"
                rows: root.rows
                value: root.band
                onSelected: function (band) {
                    root.band = band;
                }
            }
            Item {
                Layout.fillWidth: true
            }
        }
        ListView {
            id: list
            objectName: "browserList"
            Layout.fillWidth: true
            Layout.preferredHeight: 190
            orientation: ListView.Horizontal
            spacing: 14
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
                width: highlighted ? 356 : 270
                height: 164
                padding: 14
                highlighted: modelData.index === root.selected
                onClicked: root.selectRequested(modelData.index)
                background: Rectangle {
                    radius: 16
                    color: card.highlighted ? "#26302a" : "#1c1f1c"
                    border.color: card.highlighted ? "#9caf9f" : "#30ffffff"
                }
                contentItem: Item {
                    RowLayout {
                        id: channelHeading
                        width: parent.width
                        height: 32
                        spacing: 8
                        ChannelLogo {
                            logoUrl: card.modelData.logo || ""
                            Layout.preferredWidth: 56
                            Layout.preferredHeight: 32
                        }
                        Label {
                            Layout.fillWidth: true
                            text: card.modelData.label.replace(/^\d+\s+/, "")
                            color: "#b6bab6"
                            font.pixelSize: 12
                            textFormat: Text.PlainText
                            elide: Text.ElideRight
                        }
                    }
                    ChannelProgram {
                        anchors {
                            top: channelHeading.bottom
                            topMargin: 9
                            left: parent.left
                            right: parent.right
                            bottom: parent.bottom
                            bottomMargin: -6
                        }
                        program: root.programs[card.modelData.index] || null
                        now: root.now
                        emphasized: card.highlighted
                    }
                }
                Rectangle {
                    anchors.fill: parent
                    color: "transparent"
                    border.width: 2
                    radius: 16
                    border.color: "#9caf9f"
                    visible: list.activeFocus && list.currentIndex === card.index
                }
            }
            Label {
                anchors.centerIn: parent
                visible: list.count === 0
                text: qsTranslate("Viewer", "No matching channels")
                color: "#cccccc"
            }
        }
    }
}
