import {describe, expect, it} from 'vitest'
import {createDownloadProgress} from './downloadProgress.js'

describe('createDownloadProgress', () => {
    it('does not call the model downloaded when only a tiny first file has finished', () => {
        const overall = createDownloadProgress(92_887_010)

        // config.json finishes first; the 92 MB model file has not started yet.
        expect(overall({file: 'config.json', loaded: 44, total: 44})).toBeLessThan(1)
        expect(overall({file: 'onnx/model_quantized.onnx', loaded: 46_180_558, total: 92_361_116})).toBeCloseTo(49.7, 0)
        expect(overall({file: 'onnx/model_quantized.onnx', loaded: 92_361_116, total: 92_361_116})).toBeGreaterThan(99)
    })

    it('never goes down and never passes 100', () => {
        const overall = createDownloadProgress(1000)
        const seen = [
            overall({file: 'a', loaded: 600, total: 600}),
            overall({file: 'b', loaded: 100, total: 5000}),
            overall({file: 'b', loaded: 5000, total: 5000})
        ]

        expect(seen[1]).toBeGreaterThanOrEqual(seen[0])
        expect(seen[2]).toBe(100)
    })

    it('uses the sizes it has seen when the expected size is unknown', () => {
        const overall = createDownloadProgress(0)

        expect(overall({file: 'model.onnx', loaded: 250, total: 1000})).toBe(25)
    })

    it('ignores an event without sizes', () => {
        const overall = createDownloadProgress(1000)

        expect(overall({file: 'a', progress: 100})).toBeNull()
        expect(overall({file: 'a', loaded: 500, total: 1000})).toBe(50)
    })
})
