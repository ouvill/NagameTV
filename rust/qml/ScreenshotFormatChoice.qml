pragma ComponentBehavior: Bound
import QtQuick
import MinimalViewer
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
    spacing: Theme.spaceLg

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
            ? qsTranslate("Settings", "Lossless, with larger files.")
            : root.value === "jpg"
            ? qsTranslate("Settings", "Smaller files; text edges may look blurred.")
            : qsTranslate("Settings", "Compact files, with optional lossless saving.")
        color: Theme.textSecondary
        font.pixelSize: Theme.fontBody
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
        spacing: Theme.spaceSm
        RowLayout {
            Layout.fillWidth: true
            spacing: Theme.spaceLg
            Label {
                Layout.fillWidth: true
                text: root.parameterLabel
                color: Theme.textPrimary; font.pixelSize: Theme.fontControl
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
                ? qsTranslate("Settings", "Higher compression saves space but takes longer.")
                : qsTranslate("Settings", "Higher quality increases file size.")
            color: Theme.textSecondary; font.pixelSize: Theme.fontCaption
            wrapMode: Text.Wrap
        }
    }
}
