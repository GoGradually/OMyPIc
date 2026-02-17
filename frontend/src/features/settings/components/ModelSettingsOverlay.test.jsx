/** @vitest-environment jsdom */
import React from 'react'
import {describe, expect, it, vi} from 'vitest'
import {render, screen} from '@testing-library/react'
import {ModelSettingsOverlay} from './ModelSettingsOverlay.jsx'

describe('ModelSettingsOverlay', () => {
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
            />
        )

        expect(screen.getByText('기본 설정(gpt-5-nano) 기준으로 세션 1회당 약 50원이 사용됩니다.')).toBeTruthy()
        expect(screen.getByText('실제 비용은 발화 길이와 선택 모델에 따라 달라질 수 있습니다.')).toBeTruthy()
    })
})
