/** @vitest-environment jsdom */
import {afterEach, beforeEach, describe, expect, it, vi} from 'vitest'
import {pcm16ToFloat32, transcriptionTimeoutMs} from './localStt.js'

vi.mock('../../../shared/api/http.js', () => ({
    getBackendUrl: vi.fn(async () => 'http://localhost:4317')
}))

function pcm16(values) {
    return new Uint8Array(new Int16Array(values).buffer)
}

describe('local stt audio conversion', () => {
    it('converts 16-bit PCM to float samples in [-1, 1)', () => {
        const samples = pcm16ToFloat32(pcm16([0, 16384, -32768, 32767]), 16000)

        expect(Array.from(samples)).toEqual([0, 0.5, -1, 32767 / 32768])
    })

    it('resamples to the 16 kHz Whisper expects', () => {
        const fortyEight = pcm16(Array.from({length: 480}, (_, index) => index))

        const samples = pcm16ToFloat32(fortyEight, 48000)

        expect(samples.length).toBe(160)
        expect(samples[1]).toBeCloseTo(3 / 32768, 8)
    })

    it('ignores a trailing odd byte', () => {
        const bytes = new Uint8Array([0, 64, 7])

        expect(pcm16ToFloat32(bytes, 16000).length).toBe(1)
    })
})

describe('local stt model host', () => {
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
        const {loadLocalStt} = await import('./localStt.js')

        await loadLocalStt()

        expect(posted).toHaveLength(1)
        expect(posted[0].payload.config).toMatchObject({
            model: 'Xenova/whisper-base.en',
            modelHost: 'http://localhost:4317/api/local-models/',
            backendHosted: true
        })
    })

    it('keeps the configured host and path template when an override is set', async () => {
        vi.stubEnv('VITE_OMYPIC_LOCAL_STT_HOST', 'https://mirror.example/models/')
        vi.stubEnv('VITE_OMYPIC_LOCAL_STT_PATH_TEMPLATE', '{model}/main/')
        const {loadLocalStt} = await import('./localStt.js')

        await loadLocalStt()

        expect(posted[0].payload.config).toMatchObject({
            modelHost: 'https://mirror.example/models/',
            modelPathTemplate: '{model}/main/',
            backendHosted: false
        })
    })
})

describe('local stt transcription', () => {
    let posted
    let transfers

    beforeEach(() => {
        posted = []
        transfers = []
        vi.resetModules()
        vi.stubGlobal('Worker', class {
            postMessage(message, transfer) {
                posted.push(message)
                transfers.push(transfer || [])
                queueMicrotask(() => this.onmessage({data: {id: message.id, ok: true, text: message.type === 'transcribe' ? 'hello' : undefined}}))
            }

            terminate() {}
        })
    })

    afterEach(() => {
        vi.unstubAllGlobals()
    })

    it('moves the samples to the worker instead of copying them', async () => {
        const {transcribePcm16} = await import('./localStt.js')

        await expect(transcribePcm16(pcm16([1, 2, 3, 4]), 16000)).resolves.toBe('hello')

        const index = posted.findIndex((message) => message.type === 'transcribe')
        expect(transfers[index]).toEqual([posted[index].payload.samples.buffer])
    })

    it('gives a long answer more time than a short one before the worker counts as hung', () => {
        expect(transcriptionTimeoutMs(16000 * 5)).toBe(140000)
        expect(transcriptionTimeoutMs(16000 * 150)).toBe(720000)
    })
})
