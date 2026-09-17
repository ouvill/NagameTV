pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ColumnLayout {
    id: root
    required property var backend
    readonly property string value: backend.screenshot_format
    readonly property var parameters: JSON.parse(backend.screenshot_options)
    readonly property bool lossless: parameters.webp_mode === "lossless"
    readonly property int parameterValue: value === "png" ? parameters.png_compression
        : value === "jpg" ? parameters.jpg_quality : parameters.webp_quality
    readonly property string parameterLabel: value === "png"
        ? qsTranslate("Settings", "Compression level") : qsTranslate("Settings", "Image quality")
    spacing: 14

    function changeParameter(value) {
        backend.configure_screenshot_options(root.value, value, lossless);
    }
    SegmentedControl {
        objectName: "screenshotFormats"
        objectNamePrefix: "screenshotFormat_"
        Layout.fillWidth: true
        Layout.maximumWidth: 400
        options: [{value: "png", label: "PNG"}, {value: "jpg", label: "JPG"}, {value: "webp", label: "WebP"}]
        value: root.value
        onSelected: function(value) { root.backend.configure_screenshot_format(value); }
    }
    Label {
        objectName: "screenshotFormatDescription"
        Layout.fillWidth: true
        text: root.value === "png"
            ? qsTranslate("Settings", "Saves video, subtitles and comments without losing image quality. Files tend to be larger.")
            : root.value === "jpg"
            ? qsTranslate("Settings", "Saves smaller files. The edges of subtitles and comments may look blurred.")
            : qsTranslate("Settings", "Often saves smaller files than JPG at similar image quality. Lossless saving is also available.")
        color: "#b6bab6"
        font.pixelSize: 14
        wrapMode: Text.Wrap
    }
    SettingsToggle {
        objectName: "screenshotWebpLossless"
        Layout.fillWidth: true
        visible: root.value === "webp"
        text: qsTranslate("Settings", "Save without losing image quality")
        description: qsTranslate("Settings", "Prevents image quality loss from compression.")
        checked: root.lossless
        onToggled: root.backend.configure_screenshot_options("webp", root.parameters.webp_quality, checked)
    }
    ColumnLayout {
        Layout.fillWidth: true
        visible: root.value !== "webp" || !root.lossless
        spacing: 8
        RowLayout {
            Layout.fillWidth: true
            spacing: 16
            Label {
                Layout.fillWidth: true
                text: root.parameterLabel
                color: "#f4f5f3"; font.pixelSize: 16
            }
            ThemedSpinBox {
                id: number
                objectName: "screenshotParameterNumber"
                from: root.parameters.ranges[root.value].min
                to: root.parameters.ranges[root.value].max
                value: root.parameterValue
                Accessible.name: root.parameterLabel
                onValueModified: root.changeParameter(value)
            }
        }
        ThemedSlider {
            objectName: "screenshotParameterSlider"
            Layout.fillWidth: true
            from: number.from; to: number.to; stepSize: 1
            snapMode: Slider.SnapAlways
            value: root.parameterValue
            Accessible.name: root.parameterLabel
            onMoved: root.changeParameter(Math.round(value))
        }
        Label {
            objectName: "screenshotParameterDescription"
            Layout.fillWidth: true
            text: root.value === "png"
                ? qsTranslate("Settings", "Higher values spend more time saving to reduce file size. Image quality stays the same.")
                : qsTranslate("Settings", "Higher values improve image quality and increase file size.")
            color: "#9ea79f"; font.pixelSize: 13
            wrapMode: Text.Wrap
        }
    }
}
