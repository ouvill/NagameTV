pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import MinimalViewer

ScrollView {
    id: root
    property bool available: false
    property bool presenting: false
    property list<string> usedKeyGroups: []
    signal remoteKey(string domKey)
    signal dataButton()
    signal boundaryReached(int key)
    signal closeRequested
    Keys.onPressed: function(event) {
        if ([Qt.Key_Backspace, Qt.Key_X].includes(event.key)) {
            event.accepted = true;
            if (!event.isAutoRepeat) root.closeRequested();
        }
    }

    function accepts(group: string): bool {
        return available && presenting && usedKeyGroups.includes(group);
    }
    function enter() { navigation.enter(); }
    function enterLast() {
        const items = navigation.navigationItems;
        for (let index = items.length - 1; index >= 0; --index) {
            if (navigation.available(items[index])) {
                navigation.focusItem(items[index]);
                return;
            }
        }
    }
    function retainFocus() {
        if (activeFocus && (!navigation.currentItem || !navigation.currentItem.enabled))
            dataKeyButton.forceActiveFocus(Qt.TabFocusReason);
    }
    function revealFocus() {
        const flick = contentItem as Flickable;
        const item = navigation.currentItem;
        if (!flick || !item) return;
        const top = item.mapToItem(flick.contentItem, 0, 0).y - Theme.focusOutset;
        const bottom = top + item.height + 2 * Theme.focusOutset;
        const target = top < flick.contentY ? top
            : bottom > flick.contentY + flick.height ? bottom - flick.height : flick.contentY;
        flick.contentY = Math.max(0, Math.min(target, flick.contentHeight - flick.height));
    }
    onPresentingChanged: Qt.callLater(retainFocus)
    onUsedKeyGroupsChanged: Qt.callLater(retainFocus)
    onAvailableHeightChanged: Qt.callLater(revealFocus)
    clip: true
    padding: Theme.spaceSm
    contentWidth: availableWidth

    component RemoteKey: ActionButton {
        id: key
        required property string domKey
        property string group: "basic"
        property color marker: "transparent"
        objectName: "remoteKey_" + domKey
        Layout.fillWidth: true
        Layout.preferredWidth: 0
        Layout.minimumWidth: Theme.controlHeight
        leftPadding: Theme.spaceSm
        rightPadding: Theme.spaceSm
        enabled: root.accepts(group)
        onClicked: { root.remoteKey(domKey); }
        Rectangle {
            anchors { bottom: parent.bottom; bottomMargin: Theme.spaceXs; horizontalCenter: parent.horizontalCenter }
            width: parent.width - 2 * Theme.spaceMd
            height: Theme.spaceXs
            radius: Theme.indicatorRadius
            color: key.marker
            scale: key.feedbackScale
        }
    }

    DirectionalFocus {
        id: navigation
        width: root.availableWidth
        implicitHeight: layout.implicitHeight
        onCurrentItemChanged: Qt.callLater(root.revealFocus)
        onBoundaryReached: function(key) { root.boundaryReached(key); }
        // children notifies after Repeater delegates exist; itemAt() alone
        // would retain null entries from the first binding evaluation.
        navigationItems: [dataKeyButton, back, up, left, confirm, right, down, blue, red, green, yellow]
            .concat(numericKeys.children.filter(item => item instanceof ActionButton))
        initialItem: dataKeyButton
        ColumnLayout {
            id: layout
            width: parent.width
            spacing: Theme.spaceMd
            GridLayout {
                Layout.fillWidth: true
                columns: 3
                rowSpacing: Theme.spaceSm
                columnSpacing: Theme.spaceSm
                RemoteKey {
                    id: up
                    Layout.row: 0; Layout.column: 1
                    domKey: "ArrowUp"; text: "↑"
                    Accessible.name: qsTranslate("Viewer", "Up")
                }
                RemoteKey {
                    id: left
                    Layout.row: 1; Layout.column: 0
                    domKey: "ArrowLeft"; text: "←"
                    Accessible.name: qsTranslate("Viewer", "Left")
                }
                RemoteKey {
                    id: confirm
                    Layout.row: 1; Layout.column: 1
                    domKey: "Enter"; text: qsTranslate("Viewer", "OK")
                }
                RemoteKey {
                    id: right
                    Layout.row: 1; Layout.column: 2
                    domKey: "ArrowRight"; text: "→"
                    Accessible.name: qsTranslate("Viewer", "Right")
                }
                RemoteKey {
                    id: down
                    Layout.row: 2; Layout.column: 1
                    domKey: "ArrowDown"; text: "↓"
                    Accessible.name: qsTranslate("Viewer", "Down")
                }
                ActionButton {
                    id: dataKeyButton
                    objectName: "remoteDataButton"
                    Layout.row: 2; Layout.column: 0
                    Layout.fillWidth: true
                    Layout.preferredWidth: 0
                    leftPadding: Theme.spaceSm; rightPadding: Theme.spaceSm
                    text: "d"
                    Accessible.name: qsTranslate("Viewer", "Data broadcast")
                    enabled: root.available
                    onClicked: { root.dataButton(); }
                }
                RemoteKey {
                    id: back
                    Layout.row: 2; Layout.column: 2
                    domKey: "Backspace"; text: qsTranslate("Viewer", "Back")
                }
            }
            RowLayout {
                Layout.fillWidth: true
                spacing: Theme.spaceSm
                RemoteKey { id: blue; domKey: "b"; group: "data-button"; text: qsTranslate("Viewer", "Blue"); marker: Theme.remoteBlue }
                RemoteKey { id: red; domKey: "r"; group: "data-button"; text: qsTranslate("Viewer", "Red"); marker: Theme.remoteRed }
                RemoteKey { id: green; domKey: "g"; group: "data-button"; text: qsTranslate("Viewer", "Green"); marker: Theme.remoteGreen }
                RemoteKey { id: yellow; domKey: "y"; group: "data-button"; text: qsTranslate("Viewer", "Yellow"); marker: Theme.remoteYellow }
            }
            GridLayout {
                id: numericKeys
                Layout.fillWidth: true
                columns: 3
                rowSpacing: Theme.spaceSm
                columnSpacing: Theme.spaceSm
                Repeater {
                    model: 10
                    delegate: RemoteKey {
                        required property int index
                        Layout.row: Math.floor(index / 3)
                        Layout.column: index === 9 ? 1 : index % 3
                        domKey: String((index + 1) % 10)
                        group: "numeric-tuning"
                        text: domKey
                    }
                }
            }
        }
    }
}
