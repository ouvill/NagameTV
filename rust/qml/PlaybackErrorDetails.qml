pragma ComponentBehavior: Bound
import QtQuick
import MinimalViewer
import QtQuick.Controls
import QtQuick.Layouts

Popup {
    id: root
    required property string details
    parent: Overlay.overlay
    anchors.centerIn: parent
    width: Math.min(640, parent.width - 40)
    height: Math.min(420, parent.height - 80)
    modal: true
    focus: true
    padding: Theme.spaceXl
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
    background: PanelSurface {}
    onOpened: closeButton.forceActiveFocus(Qt.TabFocusReason)
    contentItem: ColumnLayout {
        Keys.onBackPressed: function(event) { if (!event.isAutoRepeat) root.close(); }
        spacing: Theme.spaceMd
        RowLayout {
            Layout.fillWidth: true
            Label {
                text: qsTranslate("Main", "Error details")
                color: Theme.textPrimary
                font.pixelSize: Theme.fontHeading
                Layout.fillWidth: true
            }
            ActionButton {
                id: closeButton
                objectName: "closePlaybackError"
                KeyNavigation.down: detailsView
                text: qsTranslate("Main", "Close")
                onClicked: root.close()
            }
        }
        ScrollView {
            id: detailsView
            objectName: "playbackErrorScroll"
            readonly property Flickable flick: contentItem as Flickable
            function scroll(direction: int) {
                const bottom = Math.max(0, flick.contentHeight - flick.height);
                flick.contentY = Math.max(0, Math.min(bottom, flick.contentY + direction * Theme.controlHeight));
            }
            Keys.onUpPressed: {
                if (flick.atYBeginning) closeButton.forceActiveFocus(Qt.TabFocusReason);
                else scroll(-1);
            }
            Keys.onDownPressed: scroll(1)
            background: ControlSurface { focused: detailsView.activeFocus }
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            contentWidth: availableWidth
            TextArea {
                objectName: "playbackErrorText"
                text: root.details
                textFormat: TextEdit.PlainText
                readOnly: true
                selectByMouse: true
                wrapMode: TextEdit.Wrap
                color: Theme.textSecondary
                font.pixelSize: Theme.fontCaption
                background: Item {}
            }
        }
    }
    Component.onCompleted: open()
}
