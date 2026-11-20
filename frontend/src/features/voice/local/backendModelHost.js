// kokoro-js downloads its voice files (voices/*.bin) from a hard-coded Hugging Face address
// (kokoro-js 1.2.1), bypassing env.remoteHost. When the backend serves the model files, this
// sends those requests to the backend as well. Only used inside the speech synthesis workers.
//
// kokoro-js does not check the response status: it stores whatever body it got in the Cache
// Storage `kokoro-voices` as a status-200 entry and reads that entry first on every later run.
// So a redirected request that fails must reject (then nothing is stored), and an entry stored
// that way by an earlier build is dropped before the voices load (dropBrokenVoiceCache).

const HUGGING_FACE = 'https://huggingface.co/'

/** The Cache Storage name kokoro-js 1.2.1 keeps its voice files in. */
export const KOKORO_VOICE_CACHE = 'kokoro-voices'

/**
 * Sends requests for huggingface.co to modelHost. With a revision, the fixed `resolve/main/`
 * address kokoro-js builds is fetched at that revision instead.
 */
export function redirectHuggingFaceFetch(scope, modelHost, revision = '') {
    const originalFetch = scope.fetch.bind(scope)
    scope.fetch = async (input, init) => {
        const url = typeof input === 'string' ? input : input instanceof URL ? input.href : input?.url
        if (typeof url !== 'string' || !url.startsWith(HUGGING_FACE)) {
            return originalFetch(input, init)
        }
        let path = url.slice(HUGGING_FACE.length)
        if (revision) {
            path = path.replace(/\/resolve\/main\//u, `/resolve/${encodeURIComponent(revision)}/`)
        }
        const target = modelHost + path
        const response = await originalFetch(typeof input === 'string' || input instanceof URL ? target : new Request(target, input), init)
        if (!response.ok) {
            throw new Error(`Model file download failed: HTTP ${response.status} for ${url}`)
        }
        return response
    }
}

// A voice file is raw float32 data. The backend's failures are JSON (or text) bodies.
export function isVoiceFile(response, size) {
    const type = (response.headers.get('content-type') || '').toLowerCase()
    if (type.includes('json') || type.startsWith('text/')) {
        return false
    }
    return size > 0 && size % Float32Array.BYTES_PER_ELEMENT === 0
}

/**
 * Removes entries from kokoro-js's voice cache that are not voice files (a failed download stored
 * as a voice). Returns how many entries it removed. Without Cache Storage it does nothing.
 */
export async function dropBrokenVoiceCache(cacheStorage) {
    if (!cacheStorage || typeof cacheStorage.open !== 'function') {
        return 0
    }
    try {
        const cache = await cacheStorage.open(KOKORO_VOICE_CACHE)
        let dropped = 0
        for (const request of await cache.keys()) {
            const response = await cache.match(request)
            const size = response ? (await response.arrayBuffer()).byteLength : 0
            if (!response || !isVoiceFile(response, size)) {
                await cache.delete(request)
                dropped += 1
            }
        }
        return dropped
    } catch (error) {
        console.warn('Cannot check the cached voice files:', error)
        return 0
    }
}

/**
 * The env.remotePathTemplate for a model: Hugging Face's own layout at the given revision when the
 * backend serves the files, else the configured template, its `{revision}` filled in as well (the
 * Kokoro library does not pass a revision on).
 */
export function modelPathTemplate({backendHosted, modelPathTemplate: template, revision}) {
    const base = backendHosted ? '{model}/resolve/{revision}/' : (template || '{model}/')
    return base.replaceAll('{revision}', encodeURIComponent(revision || 'main'))
}

/**
 * A Cache Storage that stores nothing. kokoro-js keeps its voice files under their fixed `main`
 * address in Cache Storage and reads that copy first; when the backend serves the files (already
 * on disk, at the pinned revision) the worker gives kokoro-js this instead.
 */
export const NO_CACHE_STORAGE = Object.freeze({
    open: async () => ({
        match: async () => undefined,
        put: async () => {
        },
        delete: async () => false,
        keys: async () => []
    }),
    delete: async () => false,
    has: async () => false,
    keys: async () => []
})
