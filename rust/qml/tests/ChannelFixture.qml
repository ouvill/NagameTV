import QtQuick
import MinimalViewer

// View fixtures still exercise the production Qt model and reset notifications.
ChannelModel {
    property var rows: []
    onRowsChanged: reload()
    Component.onCompleted: reload()
    function reload() {
        const values = rows.map((row, index) => ({
            index: row.index === undefined ? index : row.index,
            label: row.label || "Channel " + index,
            band: row.band || "GR",
            logo: row.logo || ""
        }));
        if (!load_test(JSON.stringify(values))) throw new Error("Invalid channel fixture");
    }
}
