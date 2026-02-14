/** @vitest-environment jsdom */
import React from 'react'
import {afterEach, describe, expect, it, vi} from 'vitest'
import {cleanup, fireEvent, render, screen} from '@testing-library/react'
import {QuestionPanel} from './QuestionPanel.jsx'

afterEach(() => {
    cleanup()
})

function buildProps(overrides = {}) {
    return {
        currentQuestionLabel: '샘플 질문',
        modeSummary: '즉시 피드백',
        onOpenQuestionGroupManager: vi.fn(),
        onOpenGroupQuestionManager: vi.fn(),
        onOpenLearningMode: vi.fn(),
        ...overrides
    }
}

describe('QuestionPanel', () => {
    it('renders learning mode and question management buttons', () => {
        render(<QuestionPanel {...buildProps()} />)

        expect(screen.getByRole('button', {name: '학습 모드 관리'})).toBeTruthy()
        expect(screen.getByRole('button', {name: '질문 그룹/태그 관리'})).toBeTruthy()
        expect(screen.getByRole('button', {name: '그룹 내 질문 관리'})).toBeTruthy()
    })

    it('calls each open handler when corresponding button is clicked', () => {
        const onOpenQuestionGroupManager = vi.fn()
        const onOpenGroupQuestionManager = vi.fn()
        const onOpenLearningMode = vi.fn()

        render(
            <QuestionPanel
                {...buildProps({
                    onOpenQuestionGroupManager,
                    onOpenGroupQuestionManager,
                    onOpenLearningMode
                })}
            />
        )

        fireEvent.click(screen.getByRole('button', {name: '학습 모드 관리'}))
        fireEvent.click(screen.getByRole('button', {name: '질문 그룹/태그 관리'}))
        fireEvent.click(screen.getByRole('button', {name: '그룹 내 질문 관리'}))

        expect(onOpenLearningMode).toHaveBeenCalledTimes(1)
        expect(onOpenQuestionGroupManager).toHaveBeenCalledTimes(1)
        expect(onOpenGroupQuestionManager).toHaveBeenCalledTimes(1)
    })
})
