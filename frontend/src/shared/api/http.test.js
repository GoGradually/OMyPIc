/** @vitest-environment jsdom */
import {afterEach, beforeEach, describe, expect, it, vi} from 'vitest'
import {getApiKey, openVoiceSession, setApiKey, verifyApiKey} from './http.js'

const KEYS = {openai: 'sk-openai', anthropic: 'sk-ant-claude'}

function okJson(body) {
    return {ok: true, json: async () => body, text: async () => JSON.stringify(body)}
}

describe('http api', () => {
    beforeEach(() => {
        window.omypic = {
            getBackendUrl: vi.fn(async () => 'http://backend'),
            getApiKey: vi.fn(async (provider) => KEYS[provider] || ''),
            setApiKey: vi.fn(async () => true)
        }
        global.fetch = vi.fn(async () => okJson({voiceSessionId: 'voice-1'}))
    })

    afterEach(() => {
        delete window.omypic
        vi.restoreAllMocks()
    })

    it('reads and writes keys per provider, defaulting to OpenAI', async () => {
        expect(await getApiKey()).toBe('sk-openai')
        expect(await getApiKey('anthropic')).toBe('sk-ant-claude')

        await setApiKey('sk-ant-new', 'anthropic')
        expect(window.omypic.setApiKey).toHaveBeenCalledWith('sk-ant-new', 'anthropic')
    })

    it('sends the Claude key only when a Claude model gives feedback', async () => {
        await openVoiceSession({sessionId: 's1', feedbackProvider: 'anthropic', feedbackEnabled: true})

        const [url, options] = global.fetch.mock.calls[0]
        expect(url).toBe('http://backend/api/voice/sessions')
        expect(options.headers['X-API-Key']).toBe('sk-openai')
        expect(options.headers['X-Feedback-API-Key']).toBe('sk-ant-claude')
    })

    it('keeps the Claude key out of OpenAI and AI-off sessions', async () => {
        await openVoiceSession({sessionId: 's1', feedbackProvider: 'openai', feedbackEnabled: true})
        await openVoiceSession({sessionId: 's1', feedbackProvider: 'anthropic', feedbackEnabled: false})

        expect(global.fetch.mock.calls[0][1].headers['X-Feedback-API-Key']).toBeUndefined()
        expect(global.fetch.mock.calls[1][1].headers['X-Feedback-API-Key']).toBeUndefined()
    })

    it('verifies a key against its own provider', async () => {
        global.fetch = vi.fn(async () => okJson({valid: true, provider: 'anthropic'}))

        await verifyApiKey('sk-ant-claude', 'claude-opus-5-5', 'anthropic')

        const body = JSON.parse(global.fetch.mock.calls[0][1].body)
        expect(body).toEqual({provider: 'anthropic', apiKey: 'sk-ant-claude', model: 'claude-opus-5-5'})
    })

    it('surfaces the backend error sentence instead of raw JSON', async () => {
        global.fetch = vi.fn(async () => ({
            ok: false,
            text: async () => '{"message":"Anthropic API key is required for Claude feedback models"}'
        }))

        await expect(openVoiceSession({sessionId: 's1'}))
            .rejects.toThrow('Anthropic API key is required for Claude feedback models')
    })
})
