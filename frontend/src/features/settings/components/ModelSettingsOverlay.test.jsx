/** @vitest-environment jsdom */
import React from 'react'
import {afterEach, describe, expect, it, vi} from 'vitest'
import {cleanup, fireEvent, render, screen} from '@testing-library/react'
import {ModelSettingsOverlay} from './ModelSettingsOverlay.jsx'

afterEach(() => {
    cleanup()
})

function buildProps(overrides = {}) {
    return {
        voiceSttModel: 'gpt-4o-mini-transcribe',
        setVoiceSttModel: vi.fn(),
        feedbackModel: 'gpt-5-nano',
        setFeedbackModel: vi.fn(),
        ttsModel: 'gpt-4o-mini-tts',
        setTtsModel: vi.fn(),
        feedbackModelOptions: ['gpt-5-nano', 'claude-opus-5-5'],
        feedbackProviderGroups: [
            {provider: 'openai', models: ['gpt-5-nano'], defaultModel: 'gpt-5-nano'},
            {provider: 'anthropic', models: ['claude-opus-5-5', 'claude-haiku-5-5'], defaultModel: 'claude-opus-5-5'}
        ],
        feedbackProvider: 'openai',
        voiceSttModelOptions: ['gpt-4o-mini-transcribe'],
        ttsModelOptions: ['gpt-4o-mini-tts'],
        apiKeyInput: '',
        setApiKeyInput: vi.fn(),
        claudeKeyInput: '',
        setClaudeKeyInput: vi.fn(),
        aiFeedbackEnabled: true,
        setAiFeedbackEnabled: vi.fn(),
        voice: 'alloy',
        setVoice: vi.fn(),
        voiceOptions: ['alloy'],
        feedbackLang: 'ko',
        setFeedbackLang: vi.fn(),
        feedbackLanguageOptions: ['ko', 'en'],
        onSaveApiKey: vi.fn(),
        onExportData: vi.fn(),
        onImportData: vi.fn(),
        ...overrides
    }
}

describe('ModelSettingsOverlay', () => {
    it('groups feedback models by provider and lets the learner pick a Claude model', () => {
        const setFeedbackModel = vi.fn()
        const {container} = render(<ModelSettingsOverlay {...buildProps({setFeedbackModel})} />)

        const groups = [...container.querySelectorAll('#feedback-model optgroup')].map((group) => group.label)
        expect(groups).toEqual(['OpenAI', 'Claude'])
        fireEvent.change(screen.getByLabelText('피드백 모델'), {target: {value: 'claude-opus-5-5'}})
        expect(setFeedbackModel).toHaveBeenCalledWith('claude-opus-5-5')
    })

    it('takes a separate Claude API key', () => {
        const setClaudeKeyInput = vi.fn()
        render(<ModelSettingsOverlay {...buildProps({
            feedbackModel: 'claude-opus-5-5',
            feedbackProvider: 'anthropic',
            setClaudeKeyInput
        })} />)

        expect(screen.getByText('Claude 모델은 아래 Claude API Key로 호출합니다.')).toBeTruthy()
        fireEvent.change(screen.getByLabelText('Claude API Key'), {target: {value: 'sk-ant-new'}})
        expect(setClaudeKeyInput).toHaveBeenCalledWith('sk-ant-new')
        expect(screen.getByLabelText('OpenAI API Key').getAttribute('type')).toBe('password')
    })

    it('disables feedback model choices while AI feedback is off', () => {
        const setAiFeedbackEnabled = vi.fn()
        render(<ModelSettingsOverlay {...buildProps({aiFeedbackEnabled: false, setAiFeedbackEnabled})} />)

        expect(screen.getByLabelText('피드백 모델').disabled).toBe(true)
        expect(screen.getByLabelText('피드백 언어').disabled).toBe(true)
        expect(screen.getByLabelText('STT 모델').disabled).toBe(false)
        fireEvent.click(screen.getByRole('switch', {name: 'AI 피드백 사용'}))
        expect(setAiFeedbackEnabled).toHaveBeenCalledWith(true)
    })

    it('shows session cost guidance for default feedback model', () => {
        render(
            <ModelSettingsOverlay
                voiceSttModel="gpt-4o-mini-transcribe"
                setVoiceSttModel={vi.fn()}
                feedbackModel="gpt-5-nano"
                setFeedbackModel={vi.fn()}
                ttsModel="gpt-4o-mini-tts"
                setTtsModel={vi.fn()}
                feedbackModelOptions={['gpt-5-nano']}
                voiceSttModelOptions={['gpt-4o-mini-transcribe']}
                ttsModelOptions={['gpt-4o-mini-tts']}
                apiKeyInput=""
                setApiKeyInput={vi.fn()}
                voice="alloy"
                setVoice={vi.fn()}
                voiceOptions={['alloy']}
                feedbackLang="ko"
                setFeedbackLang={vi.fn()}
                feedbackLanguageOptions={['ko', 'en']}
                onSaveApiKey={vi.fn()}
                onExportData={vi.fn()}
                onImportData={vi.fn()}
            />
        )

        expect(screen.getByText('기본 설정(gpt-5-nano) 기준으로 세션 1회당 약 50원이 사용됩니다.')).toBeTruthy()
        expect(screen.getByText('실제 비용은 발화 길이와 선택 모델에 따라 달라질 수 있습니다.')).toBeTruthy()
        expect(screen.getByRole('button', {name: '데이터 내보내기'})).toBeTruthy()
        expect(screen.getByRole('button', {name: '데이터 가져오기'})).toBeTruthy()
    })
})
