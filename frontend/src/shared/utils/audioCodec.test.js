import {describe, expect, it} from 'vitest'
import {float32ToInt16, fromBase64, mergeAudioChunks, toBase64} from './audioCodec.js'

describe('audioCodec', () => {
    it('converts float32 PCM to int16 with clamping', () => {
        const source = new Float32Array([-2, -1, -0.5, 0, 0.5, 1, 2])
        const converted = float32ToInt16(source)

        expect(Array.from(converted)).toEqual([-32768, -32768, -16384, 0, 16383, 32767, 32767])
    })

    it('encodes and decodes base64 audio bytes', () => {
        const bytes = new Uint8Array([0, 1, 2, 3, 253, 254, 255])
        const encoded = toBase64(bytes)
        const decoded = fromBase64(encoded)

        expect(Array.from(decoded)).toEqual(Array.from(bytes))
    })

    it('merges chunked base64 audio payloads', () => {
        const chunkA = new Uint8Array([1, 2, 3])
        const chunkB = new Uint8Array([4, 5])

        const merged = mergeAudioChunks([toBase64(chunkA), toBase64(chunkB)])

        expect(Array.from(merged)).toEqual([1, 2, 3, 4, 5])
    })
})
