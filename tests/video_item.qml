import QtQuick
import MinimalViewer
import org.freedesktop.gstreamer.Qt6GLVideoItem 1.0

Item {
    id: ordinary
    GstGLQt6VideoItem {
        id: video
        // Force a QML-generated subclass of the real native video type.
        property int subclassProperty: 1
    }
    VideoItemProbe { id: probe }
    Component.onCompleted: probe.check(ordinary, video)
}
