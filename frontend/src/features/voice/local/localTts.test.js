/** @vitest-environment jsdom */
import {afterEach, beforeEach, describe, expect, it, vi} from 'vitest'
import {redirectHuggingFaceFetch} from './backendModelHost.js'

vi.mock('../../../shared/api/http.js', () => ({
    getBackendUrl: vi.fn(async () => 'http://localhost:4317')
}))

describe('local tts model host', () => {
    let posted

    beforeEach(() => {
        posted = []
        vi.resetModules()
        vi.stubGlobal('Worker', class {
            postMessage(message) {
                posted.push(message)
                queueMicrotask(() => this.onmessage({data: {id: message.id, ok: true}}))
            }

            terminate() {}
        })
    })

    afterEach(() => {
        vi.unstubAllEnvs()
        vi.unstubAllGlobals()
    })

    it('uses the backend model cache when no host override is set', async () => {
        const {loadLocalTts} = await import('./localTts.js')

        await loadLocalTts()

        expect(posted).toHaveLength(1)
        expect(posted[0].payload.config).toMatchObject({
            model: 'onnx-community/Kokoro-82M-v1.0-ONNX',
            modelHost: 'http://localhost:4317/api/local-models/',
            revision: '1939ad2a8e416c0acfeecc08a694d14ef25f2231',
            backendHosted: true
        })
    })

    it('uses the revision a build variable names, and main for another model unless one is named', async () => {
        vi.stubEnv('VITE_OMYPIC_LOCAL_TTS_REVISION', 'abc123')
        let {loadLocalTts} = await import('./localTts.js')
        await loadLocalTts()
        expect(posted[0].payload.config.revision).toBe('abc123')

        vi.unstubAllEnvs()
        vi.resetModules()
        posted = []
        vi.stubEnv('VITE_OMYPIC_LOCAL_TTS_MODEL', 'mirror/kokoro')
        ;({loadLocalTts} = await import('./localTts.js'))
        await loadLocalTts()
        expect(posted[0].payload.config.revision).toBe('main')
    })

    it('keeps the configured host and path template when an override is set', async () => {
        vi.stubEnv('VITE_OMYPIC_LOCAL_TTS_HOST', 'https://mirror.example/models/')
        vi.stubEnv('VITE_OMYPIC_LOCAL_TTS_PATH_TEMPLATE', '{model}/main/')
        const {loadLocalTts} = await import('./localTts.js')

        await loadLocalTts()

        expect(posted[0].payload.config).toMatchObject({
            modelHost: 'https://mirror.example/models/',
            modelPathTemplate: '{model}/main/',
            backendHosted: false
        })
    })
})

describe('local tts synthesis', () => {
    let posted
    let instance

    beforeEach(() => {
        posted = []
        vi.resetModules()
        vi.stubGlobal('Worker', class {
            constructor() {
                instance = this
            }

            postMessage(message) {
                posted.push(message)
                if (message.type === 'load') {
                    queueMicrotask(() => this.onmessage({data: {id: message.id, ok: true}}))
                }
            }

            terminate() {}
        })
    })

    afterEach(() => {
        vi.unstubAllGlobals()
    })

    it('asks the worker to drop the sentences it has not started, and settles them', async () => {
        const {cancelQueuedEnglishSynthesis, synthesizeEnglish} = await import('./localTts.js')
        cancelQueuedEnglishSynthesis()
        expect(posted).toHaveLength(0)

        const first = synthesizeEnglish('First.')
        const second = synthesizeEnglish('Second.')
        await vi.waitFor(() => expect(posted.filter((message) => message.type === 'synthesize')).toHaveLength(2))
        const [one, two] = posted.filter((message) => message.type === 'synthesize')
        cancelQueuedEnglishSynthesis()

        expect(posted[posted.length - 1]).toEqual({type: 'cancel', payload: {through: two.id}})
        instance.onmessage({data: {id: one.id, ok: true, samples: new Float32Array([0.5]), sampleRate: 24000}})
        instance.onmessage({data: {id: two.id, ok: false, cancelled: true}})
        await expect(first).resolves.toMatchObject({sampleRate: 24000})
        await expect(second).rejects.toMatchObject({cancelled: true})
    })
})

describe('local tts cancel window', () => {
    let posted

    beforeEach(() => {
        posted = []
        vi.resetModules()
        vi.stubGlobal('Worker', class {
            postMessage(message) {
                posted.push(message)
                if (message.type === 'load') {
                    queueMicrotask(() => this.onmessage({data: {id: message.id, ok: true}}))
                }
            }

            terminate() {}
        })
    })

    afterEach(() => {
        vi.unstubAllGlobals()
    })

    it('never sends a sentence whose cancel came while it waited for the backend address', async () => {
        const {getBackendUrl} = await import('../../../shared/api/http.js')
        const {cancelQueuedEnglishSynthesis, loadLocalTts, synthesizeEnglish} = await import('./localTts.js')
        await loadLocalTts()
        let answerUrl
        getBackendUrl.mockImplementationOnce(() => new Promise((resolve) => {
            answerUrl = () => resolve('http://localhost:4317')
        }))

        const sentence = synthesizeEnglish('Cancelled before it was sent.')
        cancelQueuedEnglishSynthesis()
        answerUrl?.()

        await expect(sentence).rejects.toMatchObject({cancelled: true})
        expect(posted.filter((message) => message.type === 'synthesize')).toHaveLength(0)
        getBackendUrl.mockReset()
        getBackendUrl.mockImplementation(async () => 'http://localhost:4317')
    })
})

describe('local tts status', () => {
    let respond
    let instance

    beforeEach(() => {
        vi.resetModules()
        respond = (message) => ({id: message.id, ok: true})
        vi.stubGlobal('Worker', class {
            constructor() {
                instance = this
            }

            postMessage(message) {
                const reply = respond(message)
                if (reply) {
                    queueMicrotask(() => this.onmessage({data: reply}))
                }
            }

            terminate() {}
        })
    })

    afterEach(() => {
        vi.unstubAllGlobals()
    })

    it('is idle, then loading, then ready', async () => {
        const {getLocalTtsStatus, loadLocalTts} = await import('./localTts.js')
        expect(getLocalTtsStatus()).toBe('idle')

        const load = loadLocalTts()
        expect(getLocalTtsStatus()).toBe('loading')
        await load

        expect(getLocalTtsStatus()).toBe('ready')
    })

    it('tells watchers when its state changes', async () => {
        const {loadLocalTts, watchLocalTtsStatus} = await import('./localTts.js')
        const seen = []
        watchLocalTtsStatus((state) => seen.push(state.status))

        await loadLocalTts()
        instance.onerror({message: 'out of memory'})

        expect(seen).toEqual(['loading', 'ready', 'failed'])
    })

    it('is failed after a failed load or a crash, and loads again on the next call', async () => {
        respond = (message) => ({id: message.id, ok: false, message: 'HTTP 502'})
        const {getLocalTtsStatus, loadLocalTts} = await import('./localTts.js')

        await expect(loadLocalTts()).rejects.toThrow('HTTP 502')
        expect(getLocalTtsStatus()).toBe('failed')

        respond = (message) => ({id: message.id, ok: true})
        await loadLocalTts()
        expect(getLocalTtsStatus()).toBe('ready')

        instance.onerror({message: 'out of memory'})
        expect(getLocalTtsStatus()).toBe('failed')
    })
})

describe('redirectHuggingFaceFetch', () => {
    const host = 'http://localhost:4317/api/local-models/'

    function scopeWithFetch() {
        const calls = []
        return {
            calls,
            fetch: vi.fn(async (input, init) => {
                calls.push({input, init})
                return new Response('ok')
            })
        }
    }

    it('sends the fixed kokoro-js voice address to the backend', async () => {
        const scope = scopeWithFetch()
        const original = scope.fetch
        redirectHuggingFaceFetch(scope, host)

        await scope.fetch('https://huggingface.co/onnx-community/Kokoro-82M-v1.0-ONNX/resolve/main/voices/af_heart.bin', {method: 'GET'})

        expect(original).toHaveBeenCalledWith(
            'http://localhost:4317/api/local-models/onnx-community/Kokoro-82M-v1.0-ONNX/resolve/main/voices/af_heart.bin',
            {method: 'GET'}
        )
    })

    it('fetches the fixed main voice address at the pinned revision', async () => {
        const scope = scopeWithFetch()
        const original = scope.fetch
        redirectHuggingFaceFetch(scope, host, '1939ad2a8e416c0acfeecc08a694d14ef25f2231')

        await scope.fetch('https://huggingface.co/onnx-community/Kokoro-82M-v1.0-ONNX/resolve/main/voices/af_heart.bin')

        expect(original).toHaveBeenCalledWith(
            'http://localhost:4317/api/local-models/onnx-community/Kokoro-82M-v1.0-ONNX/resolve/1939ad2a8e416c0acfeecc08a694d14ef25f2231/voices/af_heart.bin',
            undefined
        )
    })

    it('rewrites URL and Request inputs and leaves other addresses alone', async () => {
        const scope = scopeWithFetch()
        redirectHuggingFaceFetch(scope, host)

        await scope.fetch(new URL('https://huggingface.co/a/b/resolve/main/c.json'))
        await scope.fetch(new Request('https://huggingface.co/a/b/resolve/main/d.json', {headers: {Range: 'bytes=0-0'}}))
        await scope.fetch('https://example.com/huggingface.co/x')
        await scope.fetch('http://localhost:4317/api/meta/models')

        expect(scope.calls[0].input).toBe('http://localhost:4317/api/local-models/a/b/resolve/main/c.json')
        expect(scope.calls[1].input.url).toBe('http://localhost:4317/api/local-models/a/b/resolve/main/d.json')
        expect(scope.calls[1].input.headers.get('range')).toBe('bytes=0-0')
        expect(scope.calls[2].input).toBe('https://example.com/huggingface.co/x')
        expect(scope.calls[3].input).toBe('http://localhost:4317/api/meta/models')
    })
})
