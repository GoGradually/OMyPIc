// The page side of one on-device speech worker (Whisper, Kokoro or the Korean model). The three
// clients differ only in their worker, their settings and their texts; this holds what they share:
// one worker, one shared load, the status, the pending requests, the cancel of queued requests,
// a watchdog against a worker that stops answering, and the release of an unused model.
//
// Status: 'idle' (never loaded, or released), 'loading', 'ready', or 'failed' (the last load failed,
// the worker crashed or stopped answering; the next call loads again in a fresh worker).

function cancelledError() {
    return Object.assign(new Error('음성 합성을 취소했습니다.'), {cancelled: true})
}

/**
 * createWorker: () => Worker (the `new Worker(new URL(...))` call stays in the client file so the
 * bundler finds it). buildConfig: async () => settings sent with every request.
 * texts: {unsupported, crashed, failed, timedOut}.
 */
export function createSpeechWorkerClient({createWorker, buildConfig, texts}) {
    let worker = null
    let nextRequestId = 0
    let loadPromise = null
    let loadedConfig = null
    let status = 'idle'
    let progress = null
    // id -> {resolve, reject, timeoutMs}; timeoutMs is null for a load (download time is not limited).
    const pending = new Map()
    const progressListeners = new Set()
    const statusListeners = new Set()
    // Moves on with every cancel; a request that was asked for before a cancel and not yet posted is
    // dropped instead of posted.
    let cancelGeneration = 0
    // Requests asked for and not yet settled, including those still waiting for the load.
    let running = 0
    let watchdog = null
    let watchdogArmedAt = 0

    function setStatus(next) {
        status = next
        if (next !== 'loading') {
            progress = null
        }
        statusListeners.forEach((listener) => listener({status, progress}))
    }

    function isSupported() {
        return typeof Worker !== 'undefined' && typeof WebAssembly !== 'undefined'
    }

    function stopWatchdog() {
        if (watchdog !== null) {
            clearTimeout(watchdog)
            watchdog = null
        }
    }

    function watchedLimit() {
        let limit = 0
        pending.forEach((request) => {
            if (request.timeoutMs) {
                limit = Math.max(limit, request.timeoutMs)
            }
        })
        return limit
    }

    // The worker runs one request at a time, so it answers one of them at least once per request
    // time. Silence for longer than the longest limit of the requests waiting means it hangs.
    function armWatchdog(fromNow) {
        const limit = watchedLimit()
        if (!limit) {
            stopWatchdog()
            return
        }
        if (fromNow || watchdog === null) {
            watchdogArmedAt = Date.now()
        }
        stopWatchdog()
        const left = Math.max(0, watchdogArmedAt + limit - Date.now())
        watchdog = setTimeout(() => {
            watchdog = null
            failAll(texts.timedOut)
        }, left)
    }

    function handleMessage(event) {
        const data = event.data || {}
        if (data.type === 'progress') {
            progressListeners.forEach((listener) => listener(data))
            if (status === 'loading' && Number.isFinite(data.progress)) {
                progress = data.progress
                setStatus('loading')
            }
            armWatchdog(true)
            return
        }
        const request = pending.get(data.id)
        if (!request) {
            return
        }
        pending.delete(data.id)
        armWatchdog(true)
        if (data.ok) {
            request.resolve(data)
        } else if (data.cancelled) {
            request.reject(cancelledError())
        } else {
            request.reject(new Error(data.message || texts.failed))
        }
    }

    // A crashed or hung worker takes the loaded model with it. It is terminated, every request
    // waiting on it fails, and the status says so: the next request starts a fresh worker.
    function failAll(message) {
        stopWatchdog()
        const requests = [...pending.values()]
        pending.clear()
        loadPromise = null
        loadedConfig = null
        if (worker) {
            worker.onmessage = null
            worker.onerror = null
            worker.terminate?.()
            worker = null
        }
        setStatus('failed')
        requests.forEach((request) => request.reject(new Error(message)))
    }

    function ensureWorker() {
        if (!worker) {
            worker = createWorker()
            worker.onmessage = handleMessage
            worker.onerror = (event) => failAll(event?.message || texts.crashed)
        }
        return worker
    }

    function post(type, payload, {transfer = [], timeoutMs = null} = {}) {
        return new Promise((resolve, reject) => {
            const id = ++nextRequestId
            pending.set(id, {resolve, reject, timeoutMs})
            ensureWorker().postMessage({id, type, payload}, transfer)
            if (timeoutMs) {
                armWatchdog(false)
            }
        })
    }

    /** Downloads (first run) and initializes the model. Safe to call repeatedly. */
    function load(onProgress) {
        if (!isSupported()) {
            setStatus('failed')
            return Promise.reject(new Error(texts.unsupported))
        }
        if (onProgress) {
            progressListeners.add(onProgress)
        }
        if (!loadPromise) {
            setStatus('loading')
            const attempt = buildConfig().then((config) => {
                loadedConfig = config
                return post('load', {config})
            }).then(() => {
                if (loadPromise === attempt) {
                    setStatus('ready')
                }
            }, (error) => {
                if (loadPromise === attempt) {
                    loadPromise = null
                    setStatus('failed')
                }
                throw error
            })
            loadPromise = attempt
        }
        return loadPromise.finally(() => {
            if (onProgress) {
                progressListeners.delete(onProgress)
            }
        })
    }

    /**
     * Loads the model if needed, then sends one request and resolves with the worker's answer.
     * makePayload(config) builds the payload; makeTransfer(payload) lists what to transfer.
     * A cancel sent while this waits for the load drops it before it reaches the worker.
     */
    async function run(type, makePayload, {timeoutMs, makeTransfer} = {}) {
        const generation = cancelGeneration
        running += 1
        try {
            await load()
            if (generation !== cancelGeneration) {
                throw cancelledError()
            }
            const payload = makePayload(loadedConfig)
            return await post(type, payload, {transfer: makeTransfer ? makeTransfer(payload) : [], timeoutMs})
        } finally {
            running -= 1
        }
    }

    /**
     * Drops every request asked for so far that the worker has not started; their promises reject
     * with an error whose `cancelled` is true. Requests asked for after this call are not affected.
     */
    function cancelQueued() {
        cancelGeneration += 1
        worker?.postMessage({type: 'cancel', payload: {through: nextRequestId}})
    }

    /** True while a load or a request is under way. */
    function isBusy() {
        return running > 0 || pending.size > 0 || status === 'loading'
    }

    /**
     * Terminates the worker and forgets the model, so its memory is returned; the next request
     * loads it again. Does nothing (returns false) while the model loads or a request is under way.
     */
    function release() {
        if (isBusy()) {
            return false
        }
        stopWatchdog()
        const hadWorker = worker !== null
        if (worker) {
            worker.onmessage = null
            worker.onerror = null
            worker.terminate?.()
            worker = null
        }
        loadPromise = null
        loadedConfig = null
        if (status !== 'idle') {
            setStatus('idle')
        }
        return hadWorker
    }

    return {
        isSupported,
        getStatus: () => status,
        watchStatus(listener) {
            statusListeners.add(listener)
            return () => statusListeners.delete(listener)
        },
        load,
        run,
        cancelQueued,
        isBusy,
        release,
        /** Whether a worker exists (for tests and measurements). */
        hasWorker: () => worker !== null
    }
}
