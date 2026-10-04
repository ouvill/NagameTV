// Compatibility policy for the pinned web-bml public API. See
// docs/data-broadcast-design.md for the evidence, limitations and removal criteria.
// Subscription + a quiet interval is observable; script readiness is not.
const settleMs = 1000;
const pollMs = 100;
function* childrenFromEnd(node) {
    for (let child = node.lastChild; child !== null; child = child.previousSibling) yield child;
}
export function hasDataButtonHandler(browser) {
    // Reverse traversal avoids upstream's synthetic arib-script wrapper error.
    for (const head of childrenFromEnd(browser.content.bmlDocument.documentElement)) {
        if (head.tagName !== "head") continue;
        for (const events of childrenFromEnd(head)) {
            if (events.tagName !== "bevent") continue;
            for (const item of childrenFromEnd(events)) {
                if (item.type === "DataButtonPressed" && item.subscribe) return true;
            }
        }
    }
    return false;
}
export class Activation {
    #task = null;
    #policy;
    #timer;
    constructor(policy, timer = globalThis) { this.#policy = policy; this.#timer = timer; }
    cancel() {
        if (this.#task !== null) this.#timer.clearTimeout(this.#task);
        this.#task = null;
    }
    update() {
        if (!this.#policy.pending()) this.cancel();
        else if (this.#task === null) this.#schedule(false, 0);
    }
    #schedule(settled, delay) {
        this.#task = this.#timer.setTimeout(() => {
            this.#task = null;
            if (!this.#policy.pending()) return;
            if (!this.#policy.subscribed()) this.#schedule(false, pollMs);
            else if (!settled) this.#schedule(true, settleMs);
            else this.#policy.deliver(); // Consume exactly one explicit open request.
        }, delay);
    }
}
