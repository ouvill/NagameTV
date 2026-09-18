pragma ComponentBehavior: Bound
import QtQuick

DropArea {
    id: root
    signal fileDropped(url file)
    signal transferDropped(string key)

    // Linux FileTransfer portal; GTK 4.6 also used the older MIME name.
    function transferFormat(formats) {
        for (const format of ["application/vnd.portal.filetransfer", "application/vnd.portal.files"])
            if (formats.indexOf(format) !== -1) return format;
        return "";
    }
    onEntered: function(drag) {
        // Reading URLs can trigger a transfer in the toolkit. Inspect only the
        // advertised formats until drop, and prefer the portal over host paths.
        drag.accepted = transferFormat(drag.formats).length > 0
            || (drag.hasUrls && drag.urls.length === 1);
    }
    onDropped: function(drop) {
        const format = transferFormat(drop.formats);
        if (format.length > 0) {
            root.transferDropped(drop.getDataAsString(format));
        } else if (drop.hasUrls && drop.urls.length === 1) {
            root.fileDropped(drop.urls[0]);
        } else {
            return;
        }
        drop.acceptProposedAction();
    }
}
