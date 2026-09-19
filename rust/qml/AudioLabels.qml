import QtQuick

QtObject {
    enum Mode { Mono = 1, DualMono = 2, Stereo = 3, Surround31 = 7, Surround32 = 8, Surround51 = 9 }
    readonly property int modeMask: 0x1f

    function language(code) {
        const names = {
            ja: "日本語", jpn: "日本語", en: "English", eng: "English",
            de: "Deutsch", deu: "Deutsch", ger: "Deutsch",
            fr: "Français", fra: "Français", fre: "Français",
            ko: "한국어", kor: "한국어", zh: "中文", zho: "中文", chi: "中文",
            ita: "Italiano", rus: "Русский", spa: "Español"
        }
        return names[code] || code
    }
    function mode(componentType) {
        switch (componentType & modeMask) {
        case AudioLabels.Mono: return qsTranslate("Viewer", "Mono")
        case AudioLabels.DualMono: return qsTranslate("Viewer", "Dual mono")
        case AudioLabels.Stereo: return qsTranslate("Viewer", "Stereo")
        case AudioLabels.Surround31: return qsTranslate("Viewer", "4-channel surround")
        case AudioLabels.Surround32: return qsTranslate("Viewer", "5-channel surround")
        case AudioLabels.Surround51: return qsTranslate("Viewer", "5.1 surround")
        default: return qsTranslate("Viewer", "Audio mode %1").arg("0x" + componentType.toString(16).padStart(2, "0"))
        }
    }
    function details(audio) {
        const languages = (audio.langs || []).map(language).join(" / ")
        const parts = languages ? [languages] : []
        if (typeof audio.componentType === "number") parts.push(mode(audio.componentType))
        if (audio.samplingRate > 0) parts.push((audio.samplingRate / 1000) + " kHz")
        return parts.join(" · ")
    }
}
