// STD-B24 5.4.13.4. Read the public BML DOM as well as the event: the pinned
// event's empty set cannot distinguish an omitted property from explicit none.
export function normalizeUsedKeyList(value) {
    const groups = value.trim().split(/\s+/).filter(Boolean);
    if (groups.includes("none")) return "";
    return [...new Set(groups.length ? groups : ["basic", "data-button"])].sort().join(" ");
}

export function usedKeyList(browser) {
    const document = browser.content.bmlDocument.documentElement;
    for (let child = document.lastChild; child !== null; child = child.previousSibling) {
        if (child.tagName === "body") return normalizeUsedKeyList(child.normalStyle.usedKeyList);
    }
    return "";
}

// A visible BML video document can have the default key mask but no target
// for key events (e.g. after d closes a menu). Keep the declared mask intact;
// the receiver only claims keys while web-bml has somewhere to deliver them.
export class KeyInput {
    #browser;
    #notify;
    #observer;
    #root = null;
    #profile = "";
    #available = false;
    constructor(browser, notify, Observer = globalThis.MutationObserver) {
        this.#browser = browser;
        this.#notify = notify;
        this.#observer = new Observer(() => this.refresh());
    }
    loaded(profile) {
        this.#observer.disconnect();
        this.#profile = profile;
        // getVideoElement is public. Its DOM root also contains the focus and
        // access-key targets; no access to web-bml's private shadowRoot is needed.
        this.#root = this.#browser.getVideoElement()?.getRootNode() ?? null;
        if (this.#root !== null) this.#observer.observe(this.#root, {
            subtree: true, childList: true, attributes: true,
            attributeFilter: ["web-bml-state", "style", "class", "accesskey"],
        });
        this.refresh();
    }
    refresh() {
        // C profile can acquire its first focus from an arrow press. Without a
        // video DOM root, retain the declared policy rather than guessing that
        // the document has no access keys.
        const available = this.#profile === "C" || this.#root === null
            || this.#browser.content.bmlDocument.currentFocus !== null
            || this.#root.querySelector("[accesskey]") !== null;
        if (available !== this.#available) {
            this.#available = available;
            this.#notify();
        }
    }
    get available() { return this.#available; }
}
