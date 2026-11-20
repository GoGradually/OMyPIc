/** @vitest-environment node */
import {afterEach, beforeEach, describe, expect, it, vi} from 'vitest'

vi.mock('@huggingface/transformers', () => ({
    env: {backends: {onnx: {wasm: {}}}},
    pipeline: vi.fn(async () => vi.fn())
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
