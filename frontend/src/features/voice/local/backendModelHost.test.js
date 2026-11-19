import {describe, expect, it, vi} from 'vitest'
import {dropBrokenVoiceCache, KOKORO_VOICE_CACHE, redirectHuggingFaceFetch} from './backendModelHost.js'

const HOST = 'http://localhost:4317/api/local-models/'
const VOICE_URL = 'https://huggingface.co/onnx-community/Kokoro-82M-v1.0-ONNX/resolve/main/voices/af_heart.bin'

function scopeAnswering(answer) {
    return {fetch: vi.fn(answer)}
}

function jsonError(status) {
    return new Response(JSON.stringify({message: 'Model download failed: upstream'}), {
        status,
        headers: {'content-type': 'application/json'}
    })
}

describe('redirectHuggingFaceFetch failures', () => {
    it.each([404, 502])('rejects a redirected request the backend answers with %i', async (status) => {
        const scope = scopeAnswering(async () => jsonError(status))
        redirectHuggingFaceFetch(scope, HOST)

        await expect(scope.fetch(VOICE_URL)).rejects.toThrow(`HTTP ${status}`)
    })

    it('passes a network error through as a rejection', async () => {
        const scope = scopeAnswering(async () => {
            throw new TypeError('Failed to fetch')
        })
        redirectHuggingFaceFetch(scope, HOST)

        await expect(scope.fetch(VOICE_URL)).rejects.toThrow('Failed to fetch')
    })

    it('returns a successful redirected response unchanged', async () => {
        const ok = new Response(new Uint8Array(8))
        const scope = scopeAnswering(async () => ok)
        redirectHuggingFaceFetch(scope, HOST)

        expect(await scope.fetch(VOICE_URL)).toBe(ok)
    })

    it('leaves the status of other requests to the caller', async () => {
        const notFound = jsonError(404)
        const scope = scopeAnswering(async () => notFound)
        redirectHuggingFaceFetch(scope, HOST)

        expect(await scope.fetch('http://localhost:4317/api/meta/models')).toBe(notFound)
    })
})

function fakeCacheStorage(entries) {
    const store = new Map(entries)
    const cache = {
        keys: vi.fn(async () => [...store.keys()].map((url) => new Request(url))),
        match: vi.fn(async (request) => store.get(request.url ?? request)?.clone()),
        delete: vi.fn(async (request) => store.delete(request.url ?? request))
    }
    return {
        store,
        storage: {open: vi.fn(async () => cache)}
    }
}

// kokoro-js stores every voice it fetched as a status-200 Response that keeps the fetched headers.
function cachedAs(body, contentType) {
    return new Response(body, {headers: contentType ? {'content-type': contentType} : {}})
}

describe('dropBrokenVoiceCache', () => {
    it('drops cached error bodies and keeps real voice files', async () => {
        const good = 'https://huggingface.co/x/resolve/main/voices/af_heart.bin'
        const json = 'https://huggingface.co/x/resolve/main/voices/af_bella.bin'
        const odd = 'https://huggingface.co/x/resolve/main/voices/am_adam.bin'
        const empty = 'https://huggingface.co/x/resolve/main/voices/bf_emma.bin'
        const {store, storage} = fakeCacheStorage([
            [good, cachedAs(new Uint8Array(1024), 'application/octet-stream')],
            [json, cachedAs('{"message":"Model download failed: ..."}', 'application/json')],
            [odd, cachedAs(new Uint8Array(1023))],
            [empty, cachedAs(new Uint8Array(0))]
        ])

        expect(await dropBrokenVoiceCache(storage)).toBe(3)

        expect(storage.open).toHaveBeenCalledWith(KOKORO_VOICE_CACHE)
        expect([...store.keys()]).toEqual([good])
    })

    it('does nothing without Cache Storage or when it fails', async () => {
        expect(await dropBrokenVoiceCache(undefined)).toBe(0)
        const logged = vi.spyOn(console, 'warn').mockImplementation(() => {})
        expect(await dropBrokenVoiceCache({open: async () => {
            throw new Error('SecurityError')
        }})).toBe(0)
        expect(logged).toHaveBeenCalledTimes(1)
        logged.mockRestore()
    })
})
