// Host protocol only. Payloads passed to web-bml retain its upstream schema.
export class SessionProtocol {
    #state = "fresh";
    #epoch = null;
    #sink;

    constructor(sink) { this.#sink = sink; }
    receive(frame) {
        if (this.#state === "stopped") return;
        if (frame.type === "fault") {
            this.#state = "stopped";
            this.#sink.fault(frame.reason);
            return;
        }
        if (frame.type === "begin") {
            if (frame.version !== 1 || typeof frame.epoch !== "string") throw new Error("Unsupported data broadcast session protocol");
            if (this.#state !== "fresh") {
                // A discontinuity needs a fresh JS realm: upstream destroy()
                // does not cancel the interpreter's timers or pending scripts.
                this.#state = "stopped";
                this.#sink.restart();
                return;
            }
            this.#epoch = frame.epoch;
            this.#state = "snapshot";
            return;
        }
        if (this.#state === "fresh") throw new Error("Data broadcast update before session begin");
        if (typeof frame.epoch !== "string") throw new Error("Missing data broadcast receive generation");
        if (frame.epoch !== this.#epoch) return; // A replaced receive generation.
        if (frame.type === "update") this.#sink.update(frame.message);
        else if (frame.type === "ready" && this.#state === "snapshot") {
            this.#state = "live";
            this.#sink.ready();
        } else throw new Error("Unexpected data broadcast session frame");
    }
}
