/** @vitest-environment node */
import {afterEach, beforeEach, describe, expect, it, vi} from 'vitest'

const mocks = vi.hoisted(() => ({generate: null, env: {backends: {onnx: {wasm: {}}}}}))

vi.mock('@huggingface/transformers', () => ({
    env: mocks.env
}))
vi.mock('kokoro-js', () => ({
    KokoroTTS: {from_pretrained: vi.fn(async () => ({generate: (...args) => mocks.generate(...args)}))}
}))

const CONFIG = {
    model: 'onnx-community/Kokoro-82M-v1.0-ONNX',
    voice: 'af_heart',
    modelHost: 'http://localhost:4317/api/local-models/',
    modelPathTemplate: '',
    revision: '1939ad2a8e416c0acfeecc08a694d14ef25f2231',
    backendHosted: true
}

describe('English tts worker synthesis', () => {
    let scope
    let spoken

    function replies() {
        return scope.postMessage.mock.calls.map(([message]) => message)
    }

    function send(id, type, payload = {}) {
        return scope.onmessage({data: {id, type, payload: {config: CONFIG, ...payload}}})
    }

    beforeEach(async () => {
        vi.resetModules()
        spoken = []
        const originalFetch = vi.fn(async () => new Response(new Uint8Array(8)))
        scope = {location: {href: 'http://localhost/worker.js'}, postMessage: vi.fn(), fetch: originalFetch, originalFetch}
        vi.stubGlobal('self', scope)
        await import('./localTtsWorker.js')
        await send(1, 'load')
    })

    afterEach(() => {
        vi.unstubAllGlobals()
    })

    it('fetches the model files and the voice at the pinned revision, without a second copy in the browser', async () => {
        expect(mocks.env.remotePathTemplate).toBe('{model}/resolve/1939ad2a8e416c0acfeecc08a694d14ef25f2231/')
        await scope.fetch('https://huggingface.co/onnx-community/Kokoro-82M-v1.0-ONNX/resolve/main/voices/af_heart.bin')
        expect(scope.originalFetch).toHaveBeenCalledWith(
            'http://localhost:4317/api/local-models/onnx-community/Kokoro-82M-v1.0-ONNX/resolve/1939ad2a8e416c0acfeecc08a694d14ef25f2231/voices/af_heart.bin',
            undefined
        )
        // kokoro-js would otherwise keep the voice under its main address in Cache Storage.
        const cache = await scope.caches.open('kokoro-voices')
        await cache.put('x', new Response('y'))
        expect(await cache.match('x')).toBeUndefined()
    })

    it('reports the download of the whole model, not of the small file that finishes first', async () => {
        const {KokoroTTS} = await import('kokoro-js')
        vi.resetModules()
        scope = {location: {href: 'http://localhost/worker.js'}, postMessage: vi.fn(), fetch: vi.fn()}
        vi.stubGlobal('self', scope)
        KokoroTTS.from_pretrained.mockImplementationOnce(async (_model, options) => {
            options.progress_callback({status: 'progress', file: 'config.json', progress: 100, loaded: 44, total: 44})
            options.progress_callback({
                status: 'progress',
                file: 'onnx/model_quantized.onnx',
                progress: 50,
                loaded: 46_180_558,
                total: 92_361_116
            })
            return {generate: (...args) => mocks.generate(...args)}
        })
        await import('./localTtsWorker.js')

        await scope.onmessage({data: {id: 1, type: 'load', payload: {config: {...CONFIG, downloadBytes: 92_887_010}}}})

        const progress = replies().filter((message) => message.type === 'progress').map((message) => message.progress)
        expect(progress).toHaveLength(2)
        expect(progress[0]).toBeLessThan(1)
        expect(progress[1]).toBeCloseTo(49.7, 0)
    })

    it('redirects the voice address once, however often the load is tried again', async () => {
        const {KokoroTTS} = await import('kokoro-js')
        vi.resetModules()
        const originalFetch = vi.fn(async () => new Response(new Uint8Array(8)))
        scope = {location: {href: 'http://localhost/worker.js'}, postMessage: vi.fn(), fetch: originalFetch, originalFetch}
        vi.stubGlobal('self', scope)
        KokoroTTS.from_pretrained.mockRejectedValueOnce(new Error('offline'))
        await import('./localTtsWorker.js')

        await send(1, 'load')
        expect(replies()[0]).toMatchObject({id: 1, ok: false})
        const redirected = scope.fetch
        await send(2, 'load')

        expect(replies()[1]).toEqual({id: 2, ok: true})
        expect(scope.fetch).toBe(redirected)
    })

it('synthesizes one sentence at a time and drops those still waiting when a cancel arrives', async () => {
        const releases = []
        mocks.generate = (text) => {
            spoken.push(text)
            return new Promise((resolve) => {
                releases.push(() => resolve({audio: new Float32Array([0.5]), sampling_rate: 24000}))
            })
        }

        const running = [send(2, 'synthesize', {text: 'First.'}), send(3, 'synthesize', {text: 'Second.'})]
        await vi.waitFor(() => expect(spoken).toHaveLength(1))
        await new Promise((resolve) => setTimeout(resolve, 0))
        expect(spoken).toEqual(['First.'])

        await scope.onmessage({data: {type: 'cancel', payload: {through: 3}}})
        running.push(send(4, 'synthesize', {text: 'New question.'}))
        releases[0]()
        await vi.waitFor(() => expect(spoken).toHaveLength(2))
        releases[1]()
        await Promise.all(running)

        expect(spoken).toEqual(['First.', 'New question.'])
        expect(replies().slice(1).map(({id, ok, cancelled}) => ({id, ok, cancelled}))).toEqual([
            {id: 2, ok: true, cancelled: undefined},
            {id: 3, ok: false, cancelled: true},
            {id: 4, ok: true, cancelled: undefined}
        ])
        expect(replies()[3]).toMatchObject({sampleRate: 24000})
    })
})
