import {describe, expect, it} from 'vitest'
import {
    FEEDBACK_LANGUAGE_LABELS,
    getModeSummary,
    getModelLabel,
    MODEL_LABELS
} from './models.js'

describe('models constants', () => {
    it('uses question-group wording for continuous mode summary', () => {
        expect(getModeSummary('CONTINUOUS', 3)).toContain('질문 그룹 단위')
    })

    it('exposes display labels for known model IDs', () => {
        expect(MODEL_LABELS['gpt-5-mini']).toContain('균형형')
        expect(MODEL_LABELS['gpt-4o-mini-transcribe']).toContain('기본')
        expect(MODEL_LABELS['gpt-4o-mini-tts']).toContain('기본')
    })

    it('returns model ID when label is unknown', () => {
        expect(getModelLabel('custom-model')).toBe('custom-model')
        expect(getModelLabel('')).toBe('')
    })

    it('keeps language labels for UI rendering', () => {
        expect(FEEDBACK_LANGUAGE_LABELS.ko).toBe('한국어')
        expect(FEEDBACK_LANGUAGE_LABELS.en).toBe('English')
    })
})
