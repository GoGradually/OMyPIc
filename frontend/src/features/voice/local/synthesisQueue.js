// One synthesis at a time in a speech worker, in the order the requests arrived. Several
// sentences arrive together, and the model would otherwise run them all at once.
//
// When the learner stops, the client sends a cancel naming the last request id it has sent; a
// request with that id or lower that has not started yet is dropped instead of run, so speech for
// the next session does not wait behind it. A request that already runs finishes.

export function createSynthesisQueue() {
    let tail = Promise.resolve()
    let cancelledThrough = 0
    return {
        /** Runs synthesize() after every earlier request; resolves to null when it was dropped. */
        enqueue(id, synthesize) {
            const result = tail.then(() => (id <= cancelledThrough ? null : synthesize()))
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
