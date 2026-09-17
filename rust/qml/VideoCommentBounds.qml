import QtQuick

Item {
    id: root
    required property real viewportWidth
    required property real viewportHeight
    required property real aspectRatio
    property bool evaluationWide: false
    // JP4734471 claims 1/7/9: keep comments out of non-video margins. Estimated
    // expiry 2026-12-11; registry status unverified, never auto-enable by date.
    // qml6glsink uses integer fitted dimensions, including pixel aspect ratio.
    readonly property bool known: isFinite(aspectRatio) && aspectRatio > 0
    width: evaluationWide ? viewportWidth : known ? Math.floor(Math.min(viewportWidth, viewportHeight * aspectRatio)) : 0
    height: evaluationWide ? Math.min(viewportHeight, viewportWidth * 9 / 16) : known ? Math.floor(Math.min(viewportHeight, viewportWidth / aspectRatio)) : 0
    x: Math.floor((viewportWidth - width) / 2)
    y: Math.floor((viewportHeight - height) / 2)
    clip: true
}
