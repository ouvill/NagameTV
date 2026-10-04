// Presentation belongs to the browser adapter; reception belongs to Rust.
export const Presentation = Object.freeze({
    Opening: "opening",
    Presenting: "presenting",
    Transitioning: "transitioning",
    Standby: "standby",
    Unavailable: "unavailable",
});

// Publish once after a group of synchronous state/layout changes. An idle page
// does no polling, layout measurement, or WebChannel work.
export class PresentationPublisher {
    #read;
    #send;
    #schedule;
    #pending = false;
    #previous = null;
    constructor(read, send, schedule = callback => globalThis.queueMicrotask(callback)) {
        this.#read = read;
        this.#send = send;
        this.#schedule = schedule;
    }
    update() {
        if (this.#pending) return;
        this.#pending = true;
        this.#schedule(() => {
            this.#pending = false;
            const next = this.#read();
            const old = this.#previous;
            const a = old?.videoRect;
            const b = next.videoRect;
            if (old !== null && old.revision === next.revision && old.state === next.state
                && old.usedKeyList === next.usedKeyList
                && old.inputAvailable === next.inputAvailable
                && old.documentUrl === next.documentUrl
                && (a === b || (a != null && b != null && a.x === b.x && a.y === b.y
                    && a.width === b.width && a.height === b.height))) return;
            this.#previous = next;
            this.#send(next);
        });
    }
}

export class PresentationState {
    state = Presentation.Opening;
    loading = true;
    invisible = true;
    activationPending = true;
    receiving = false;
    available = null;
    synchronized = false;

    reset(activate) {
        this.state = activate ? Presentation.Opening : Presentation.Standby;
        this.loading = true;
        this.invisible = true;
        this.activationPending = activate;
        this.receiving = false;
        this.available = null;
        this.synchronized = false;
    }

    open() {
        this.state = Presentation.Opening;
        this.activationPending = true;
        this.settle();
    }

    navigation(loading) {
        this.loading = loading;
        if (loading && this.state === Presentation.Presenting) {
            this.state = Presentation.Transitioning;
        }
        this.settle();
    }

    visibility(invisible) {
        this.invisible = invisible;
        this.settle();
    }

    reception(receiving) {
        this.receiving = receiving;
    }

    availability(available) {
        this.available = available;
        if (this.state === Presentation.Unavailable) this.state = Presentation.Opening;
        this.settle();
    }

    settle() {
        if (this.available === false) {
            this.state = Presentation.Unavailable;
            return;
        }
        // A document's load event precedes its scripts and onload handler.
        // Only Indicator.setUrl(..., false) marks navigation as complete.
        if (this.loading || this.activationPending) return;
        if (!this.invisible) {
            this.state = Presentation.Presenting;
        } else {
            this.state = Presentation.Standby;
        }
    }

    get needsActivation() {
        // A startup document can be visible but show only the television video.
        // Visibility does not mean that the user's data-button request was sent.
        return this.synchronized && this.available !== false && this.activationPending && !this.loading && !this.receiving;
    }

    takeActivation() {
        if (!this.needsActivation) return false;
        this.activationPending = false;
        this.settle();
        return true;
    }
}
