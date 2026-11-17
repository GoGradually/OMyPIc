import {describe, expect, it} from 'vitest'
import {
    FEEDBACK_LANGUAGE_LABELS,
    getFeedbackProvider,
    getModeSummary,
    getModelLabel,
    getProviderLabel,
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

    it('labels Claude models and providers', () => {
        expect(getModelLabel('claude-opus-5-5')).toContain('Claude Opus 5.5')
        expect(getProviderLabel('anthropic')).toBe('Claude')
        expect(getProviderLabel('openai')).toBe('OpenAI')
    })

    it('resolves the provider of a feedback model from the server groups', () => {
        const groups = [
            {provider: 'openai', models: ['gpt-5-nano']},
            {provider: 'anthropic', models: ['claude-haiku-5-5']}
        ]
        expect(getFeedbackProvider('claude-haiku-5-5', groups)).toBe('anthropic')
        expect(getFeedbackProvider('gpt-5-nano', groups)).toBe('openai')
    })

    it('falls back to the model id when provider groups are unavailable', () => {
        expect(getFeedbackProvider('claude-sonnet-5-5')).toBe('anthropic')
        expect(getFeedbackProvider('gpt-4o')).toBe('openai')
        expect(getFeedbackProvider('')).toBe('openai')
    })
})
