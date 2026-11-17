/** @vitest-environment jsdom */
import React from 'react'
import {afterEach, describe, expect, it, vi} from 'vitest'
import {cleanup, render, screen} from '@testing-library/react'
import {SummaryPanel} from './SummaryPanel.jsx'

afterEach(() => {
    cleanup()
})

describe('SummaryPanel', () => {
    it('shows question group count label', () => {
        render(
            <SummaryPanel
                feedbackLang="ko"
                voice="alloy"
                voiceSttModel="gpt-4o-mini-transcribe"
                feedbackModel="gpt-4o-mini"
                ttsModel="gpt-4o-mini-tts"
                enabledRulebookCount={2}
                questionGroupCount={3}
                onOpenSettings={vi.fn()}
            />
        )

        expect(screen.getByText('질문 그룹')).toBeTruthy()
        expect(screen.getByText('3개')).toBeTruthy()
        expect(screen.getByText('TTS 모델')).toBeTruthy()
        expect(screen.getByText('gpt-4o-mini-tts')).toBeTruthy()
    })

    it('shows that AI feedback is off', () => {
        render(
            <SummaryPanel
                feedbackLang="ko"
                voice="alloy"
                voiceSttModel="gpt-4o-mini-transcribe"
                feedbackModel="claude-opus-5-5"
                aiFeedbackEnabled={false}
                ttsModel="gpt-4o-mini-tts"
                enabledRulebookCount={0}
                questionGroupCount={1}
                onOpenSettings={vi.fn()}
            />
        )

        expect(screen.getByText('AI 피드백').nextElementSibling.textContent).toBe('꺼짐')
        expect(screen.getByText('claude-opus-5-5')).toBeTruthy()
    })
})
