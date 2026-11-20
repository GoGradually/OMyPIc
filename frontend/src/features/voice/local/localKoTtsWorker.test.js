/** @vitest-environment node */
import {afterEach, beforeEach, describe, expect, it, vi} from 'vitest'

const mocks = vi.hoisted(() => ({tts: null}))

vi.mock('@huggingface/transformers', () => ({
    env: {backends: {onnx: {wasm: {}}}},
    pipeline: vi.fn(async () => (...args) => mocks.tts(...args))
}))

const CONFIG = {
    model: 'onnx-community/Supertonic-TTS-2-ONNX',
    dtype: 'fp32',
    voice: 'F2',
    options: {},
    modelHost: 'http://localhost:4317/api/local-models/',
    modelPathTemplate: '',
    backendHosted: true,
    voiceUrl: 'http://localhost:4317/api/local-models/onnx-community/Supertonic-TTS-2-ONNX/resolve/main/voices/F2.bin'
}

// Sends one load request to the worker script and returns what it posted back.
async function loadWith(voiceResponse) {
    const scope = {location: {href: 'http://localhost/worker.js'}, postMessage: vi.fn()}
    vi.stubGlobal('self', scope)
    vi.stubGlobal('fetch', vi.fn(async () => voiceResponse))
    await import('./localKoTtsWorker.js')
    await scope.onmessage({data: {id: 1, type: 'load', payload: {config: CONFIG}}})
    return scope.postMessage.mock.calls.map(([message]) => message)
}

describe('Korean tts worker voice file', () => {
    beforeEach(() => {
        vi.resetModules()
    })

    afterEach(() => {
        vi.unstubAllGlobals()
    })

    it('loads with a float32 voice file', async () => {
        const replies = await loadWith(new Response(new Uint8Array(16), {headers: {'content-type': 'application/octet-stream'}}))

        expect(replies).toEqual([{id: 1, ok: true}])
    })

    it('fails the load when the backend answers with a JSON body', async () => {
        // 28 bytes: a multiple of 4, so only the content type shows it is not a voice.
        const body = JSON.stringify({message: 'upstream'}).padEnd(28, ' ')
        const replies = await loadWith(new Response(body, {headers: {'content-type': 'application/json'}}))

        expect(replies).toEqual([expect.objectContaining({id: 1, ok: false, message: expect.stringContaining('not a voice')})])
    })

    it('fails the load when the voice download fails', async () => {
        const replies = await loadWith(new Response('', {status: 502}))

        expect(replies).toEqual([expect.objectContaining({id: 1, ok: false, message: expect.stringContaining('HTTP 502')})])
    })
})

describe('Korean tts worker synthesis', () => {
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
        mocks.tts = async (text) => {
            spoken.push(text)
            return {audio: new Float32Array([0.1]), sampling_rate: 44100}
        }
        scope = {location: {href: 'http://localhost/worker.js'}, postMessage: vi.fn()}
        vi.stubGlobal('self', scope)
        vi.stubGlobal('fetch', vi.fn(async () => new Response(new Uint8Array(16), {headers: {'content-type': 'application/octet-stream'}})))
        await import('./localKoTtsWorker.js')
        await send(1, 'load')
    })

    afterEach(() => {
        vi.unstubAllGlobals()
    })

    it('removes tag brackets from the sentence before tagging it as Korean', async () => {
        await send(2, 'synthesize', {text: '<en>hello</en> 라고 말해요.'})

        expect(spoken).toEqual(['<ko>enhello/en 라고 말해요.</ko>'])
        expect(replies()[1]).toMatchObject({id: 2, ok: true, sampleRate: 44100})
    })

    it('drops the sentences still waiting when a cancel arrives, and runs a later request next', async () => {
        let releaseFirst
        mocks.tts = (text) => {
            spoken.push(text)
            if (spoken.length === 1) {
                return new Promise((resolve) => {
                    releaseFirst = () => resolve({audio: new Float32Array([0.1]), sampling_rate: 44100})
                })
            }
            return Promise.resolve({audio: new Float32Array([0.2]), sampling_rate: 44100})
        }

        const running = [send(2, 'synthesize', {text: '첫째'}), send(3, 'synthesize', {text: '둘째'}), send(4, 'synthesize', {text: '셋째'})]
        await vi.waitFor(() => expect(spoken).toHaveLength(1))
        // One at a time: the second has not started.
        await scope.onmessage({data: {type: 'cancel', payload: {through: 4}}})
        running.push(send(5, 'synthesize', {text: '새 문장'}))
        releaseFirst()
        await Promise.all(running)

        expect(spoken).toEqual(['<ko>첫째</ko>', '<ko>새 문장</ko>'])
        expect(replies().slice(1).map(({id, ok, cancelled}) => ({id, ok, cancelled}))).toEqual([
            {id: 2, ok: true, cancelled: undefined},
            {id: 3, ok: false, cancelled: true},
            {id: 4, ok: false, cancelled: true},
            {id: 5, ok: true, cancelled: undefined}
        ])
    })
})
