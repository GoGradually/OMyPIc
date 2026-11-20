// One synthesis at a time in a speech worker, in the order the requests arrived. Several
// sentences arrive together, and the model would otherwise run them all at once.
//
// When the learner stops, the client sends a cancel naming the last request id it has sent; a
// request with that id or lower that has not started yet is dropped instead of run, so speech for
// the next session does not wait behind it. A request that already runs finishes.

// A synthesis keeps the worker busy (the model runs on this thread), and the steps between one
// synthesis and the next are all promise callbacks, which run before any message. Waiting for a
// timer before each request lets a cancel that arrived meanwhile be read first.
function afterPendingMessages() {
    return new Promise((resolve) => setTimeout(resolve, 0))
}

export function createSynthesisQueue() {
    let tail = Promise.resolve()
    let cancelledThrough = 0
    return {
        /** Runs synthesize() after every earlier request; resolves to null when it was dropped. */
        enqueue(id, synthesize) {
            const result = tail.then(afterPendingMessages).then(() => (id <= cancelledThrough ? null : synthesize()))
            tail = result.catch(() => {
            })
            return result
        },
        /** Drops the requests up to and including this id that have not started. */
        cancelThrough(id) {
            cancelledThrough = Math.max(cancelledThrough, Number(id) || 0)
        }
    }
}
