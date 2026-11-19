pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import MinimalViewer

ColumnLayout {
    id: root
    required property var backend
    required property string kind
    property bool busy: false
    readonly property string status: backend.lan_search_kind === kind ? backend.lan_search_status : "idle"
    readonly property list<Item> navigationItems: [search, ...rows.children.filter(child => child !== results)]
    signal serverSelected(string url, int focusReason)
    spacing: Theme.spaceSm

    function start() { backend.search_lan(kind); }
    function stop() { if (status !== "idle") backend.cancel_lan_search(kind); }
    function focusSearch() { search.forceActiveFocus(Qt.TabFocusReason); }

    ActionButton {
        id: search
        objectName: "lanSearch_" + root.kind
        enabled: !root.busy || root.status === "searching"
        text: root.status === "searching" ? qsTranslate("Connection", "Cancel search")
            : root.status === "idle" ? qsTranslate("Connection", "Search local network")
            : qsTranslate("Connection", "Search again")
        onClicked: {
            if (root.status === "searching") root.stop();
            else root.start();
        }
    }
    Label {
        Layout.fillWidth: true
        visible: text.length > 0
        text: {
            switch (root.status) {
            case "searching": return qsTranslate("Connection", "Searching the local network…");
            case "partial": return qsTranslate("Connection", "Search ended early. Try again or enter the URL.");
            case "failed": return qsTranslate("Connection", "Could not search the local network.");
            case "finished": return results.count === 0 ? qsTranslate("Connection", "No compatible servers found.") : "";
            default: return "";
            }
        }
        color: Theme.textSecondary
        font.pixelSize: Theme.fontBody
        wrapMode: Text.Wrap
    }
    ColumnLayout {
        id: rows
        Layout.fillWidth: true
        spacing: Theme.spaceSm
        visible: results.count > 0
        Repeater {
            id: results
            model: root.status === "finished" || root.status === "partial" ? root.backend.lan_discovery_model : null
            delegate: ActionButton {
                id: choice
                required property int index
                required property string serverUrl
                required property string productName
                required property string serverVersion
                objectName: "lanResult_" + root.kind + "_" + index
                Layout.fillWidth: true
                enabled: !root.busy
                textAlignment: Text.AlignLeft
                wrapText: true
                text: (serverVersion.length > 0 ? productName + " " + serverVersion : productName) + "\n" + serverUrl
                onClicked: root.serverSelected(choice.serverUrl, choice.focusVisible ? Qt.TabFocusReason : Qt.OtherFocusReason)
            }
        }
    }
}
