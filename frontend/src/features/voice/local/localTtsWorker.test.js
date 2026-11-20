/** @vitest-environment node */
import {afterEach, beforeEach, describe, expect, it, vi} from 'vitest'

const mocks = vi.hoisted(() => ({generate: null}))

vi.mock('@huggingface/transformers', () => ({
    env: {backends: {onnx: {wasm: {}}}}
}))
vi.mock('kokoro-js', () => ({
    KokoroTTS: {from_pretrained: vi.fn(async () => ({generate: (...args) => mocks.generate(...args)}))}
}))

const CONFIG = {
    model: 'onnx-community/Kokoro-82M-v1.0-ONNX',
    voice: 'af_heart',
    modelHost: 'http://localhost:4317/api/local-models/',
    modelPathTemplate: '',
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
        scope = {location: {href: 'http://localhost/worker.js'}, postMessage: vi.fn(), fetch: vi.fn()}
        vi.stubGlobal('self', scope)
        await import('./localTtsWorker.js')
        await send(1, 'load')
    })

    afterEach(() => {
        vi.unstubAllGlobals()
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
