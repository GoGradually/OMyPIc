/** @vitest-environment jsdom */
import React from 'react'
import {describe, expect, it, vi} from 'vitest'
import {render, screen} from '@testing-library/react'
import {SummaryPanel} from './SummaryPanel.jsx'

describe('SummaryPanel', () => {
    it('shows question group count label', () => {
        render(
            <SummaryPanel
                feedbackLang="ko"
                voice="alloy"
                realtimeConversationModel="gpt-realtime-mini"
                realtimeSttModel="gpt-4o-mini-transcribe"
                feedbackModel="gpt-realtime-mini"
                enabledRulebookCount={2}
                questionGroupCount={3}
                onOpenSettings={vi.fn()}
            />
        )

        expect(screen.getByText('질문 그룹')).toBeTruthy()
        expect(screen.getByText('3개')).toBeTruthy()
    })
})
