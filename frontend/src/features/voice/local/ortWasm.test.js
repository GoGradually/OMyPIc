/** @vitest-environment node */
import {afterEach, beforeEach, describe, expect, it, vi} from 'vitest'

// Every worker shares the transformers `env`, so each test reads the paths its worker set.
const env = {backends: {onnx: {wasm: {}}}}

vi.mock('@huggingface/transformers', () => ({
    env,
    pipeline: vi.fn(async () => vi.fn())
}))
vi.mock('kokoro-js', () => ({
    KokoroTTS: {from_pretrained: vi.fn(async () => ({generate: vi.fn()}))}
}))

const CONFIG = {
    model: 'some/model',
    dtype: 'fp32',
    voice: 'F2',
    options: {},
    modelHost: 'http://localhost:4317/api/local-models/',
    modelPathTemplate: '',
    backendHosted: true,
    voiceUrl: 'http://localhost:4317/api/local-models/some/model/resolve/main/voices/F2.bin'
}

async function loadWorker(file) {
    const scope = {location: {href: 'http://localhost/assets/worker.js'}, postMessage: vi.fn(), fetch: vi.fn()}
    vi.stubGlobal('self', scope)
    vi.stubGlobal('fetch', vi.fn(async () => new Response(new Uint8Array(16), {headers: {'content-type': 'application/octet-stream'}})))
    await import(file)
    await scope.onmessage({data: {id: 1, type: 'load', payload: {config: CONFIG}}})
    expect(scope.postMessage).toHaveBeenCalledWith({id: 1, ok: true})
    return env.backends.onnx.wasm.wasmPaths
}

describe('ONNX Runtime build of the speech workers', () => {
    beforeEach(() => {
        vi.resetModules()
        env.backends.onnx.wasm = {}
    })

    afterEach(() => {
        vi.unstubAllGlobals()
    })

    it.each([
        ['speech recognizer', './localSttWorker.js'],
        ['English synthesizer', './localTtsWorker.js'],
        ['Korean synthesizer', './localKoTtsWorker.js']
    ])('the %s loads the bundled plain (not Asyncify) build', async (_label, file) => {
        const paths = await loadWorker(file)

        expect(paths.mjs).toMatch(/^http:\/\/localhost\/.*ort-wasm-simd-threaded\.mjs$/)
        expect(paths.wasm).toMatch(/^http:\/\/localhost\/.*ort-wasm-simd-threaded\.wasm$/)
        expect(`${paths.mjs} ${paths.wasm}`).not.toContain('asyncify')
    })
})
