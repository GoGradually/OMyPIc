/** @vitest-environment node */
import {afterEach, beforeEach, describe, expect, it, vi} from 'vitest'

const mocks = vi.hoisted(() => ({sessions: null, env: {backends: {onnx: {wasm: {}}}}}))

vi.mock('@huggingface/transformers', () => ({
    env: mocks.env,
    pipeline: vi.fn(async () => {
        const transcriber = async () => {
            // What the library does: run each session with its inputs only.
            await mocks.sessions.model.run({input_features: 'features'})
            await mocks.sessions.decoder_model_merged.run({input_ids: 'ids'})
            return {text: ' hello '}
        }
        transcriber.model = {sessions: mocks.sessions}
        return transcriber
    })
}))

function fakeSession(outputNames) {
    const calls = []
    return {
        outputNames,
        calls,
        run: vi.fn(async (feeds, fetches) => {
            calls.push(fetches)
            return {}
        })
    }
}

const CONFIG = {
    model: 'Xenova/whisper-base.en',
    revision: '95bf40a508535962c6483ead40270b2e32267508',
    modelHost: 'http://localhost:4317/api/local-models/',
    modelPathTemplate: '',
    backendHosted: true
}

describe('Whisper worker', () => {
    let scope

    beforeEach(async () => {
        vi.resetModules()
        mocks.sessions = {
            model: fakeSession(['last_hidden_state', 'encoder_attentions.0', 'encoder_attentions.1']),
            decoder_model_merged: fakeSession(['logits', 'present.0.decoder.key', 'decoder_attentions.0', 'cross_attentions.0'])
        }
        scope = {location: {href: 'http://localhost/worker.js'}, postMessage: vi.fn()}
        vi.stubGlobal('self', scope)
        await import('./localSttWorker.js')
    })

    afterEach(() => {
        vi.unstubAllGlobals()
    })

    it('asks the model for its results only, not the attention maps nobody reads', async () => {
        await scope.onmessage({data: {id: 1, type: 'load', payload: {config: CONFIG}}})
        await scope.onmessage({data: {id: 2, type: 'transcribe', payload: {config: CONFIG, samples: new Float32Array(16000)}}})

        expect(scope.postMessage).toHaveBeenLastCalledWith({id: 2, ok: true, text: 'hello'})
        expect(mocks.sessions.model.calls).toEqual([['last_hidden_state']])
        expect(mocks.sessions.decoder_model_merged.calls).toEqual([['logits', 'present.0.decoder.key']])
    })

    it('loads the pinned revision', async () => {
        const {pipeline} = await import('@huggingface/transformers')
        await scope.onmessage({data: {id: 1, type: 'load', payload: {config: CONFIG}}})

        expect(mocks.env.remotePathTemplate).toBe('{model}/resolve/95bf40a508535962c6483ead40270b2e32267508/')
        expect(pipeline).toHaveBeenCalledWith('automatic-speech-recognition', CONFIG.model, expect.objectContaining({revision: CONFIG.revision}))
    })
})
