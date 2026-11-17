/** @vitest-environment jsdom */
import {describe, expect, it} from 'vitest'
import {pcm16ToFloat32} from './localStt.js'

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
