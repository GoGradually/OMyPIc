import {describe, expect, it} from 'vitest'
import {
    DEFAULT_FEEDBACK_MODEL,
    DEFAULT_TTS_MODEL,
    DEFAULT_VOICE_STT_MODEL,
    FEEDBACK_MODELS,
    getModeSummary,
    TTS_MODELS,
    VOICE_STT_MODELS
} from './models.js'

describe('models constants', () => {
    it('uses question-group wording for continuous mode summary', () => {
        expect(getModeSummary('CONTINUOUS', 3)).toContain('질문 그룹 단위')
    })

    it('keeps only cost-acceptable feedback model options', () => {
        expect(FEEDBACK_MODELS).not.toContain('gpt-5-pro')
        expect(FEEDBACK_MODELS).not.toContain('gpt-5.2')
        expect(FEEDBACK_MODELS).not.toContain('gpt-5.1')
        expect(FEEDBACK_MODELS).not.toContain('gpt-5')
        expect(FEEDBACK_MODELS).toContain('gpt-5-mini')
        expect(FEEDBACK_MODELS).toContain('gpt-5-nano')
        expect(FEEDBACK_MODELS).toContain('gpt-4.1')
        expect(FEEDBACK_MODELS).toContain(DEFAULT_FEEDBACK_MODEL)
    })

    it('uses gpt-5-nano as default feedback model', () => {
        expect(DEFAULT_FEEDBACK_MODEL).toBe('gpt-5-nano')
    })

    it('keeps default model values present in selectable lists', () => {
        expect(VOICE_STT_MODELS).toContain(DEFAULT_VOICE_STT_MODEL)
        expect(TTS_MODELS).toContain(DEFAULT_TTS_MODEL)
    })
})
