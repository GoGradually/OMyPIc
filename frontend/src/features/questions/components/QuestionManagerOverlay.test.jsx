/** @vitest-environment jsdom */
import React from 'react'
import {afterEach, describe, expect, it, vi} from 'vitest'
import {cleanup, render, screen} from '@testing-library/react'
import {QuestionManagerOverlay} from './QuestionManagerOverlay.jsx'

afterEach(() => {
    cleanup()
})

function buildProps(overrides = {}) {
    return {
        activeGroupId: '',
        newGroupName: '',
        setNewGroupName: vi.fn(),
        newGroupTagsInput: '',
        setNewGroupTagsInput: vi.fn(),
        questionGroups: [],
        setActiveGroupId: vi.fn(),
        createGroup: vi.fn(),
        deleteGroup: vi.fn(),
        newQuestion: '',
        setNewQuestion: vi.fn(),
        newQuestionType: '',
        setNewQuestionType: vi.fn(),
        addQuestion: vi.fn(),
        activeQuestionGroup: null,
        editingQuestionId: '',
        editingQuestionText: '',
        setEditingQuestionText: vi.fn(),
        editingQuestionType: '',
        setEditingQuestionType: vi.fn(),
        startEditQuestion: vi.fn(),
        saveEditedQuestion: vi.fn(),
        cancelEditQuestion: vi.fn(),
        removeQuestion: vi.fn(),
        ...overrides
    }
}

describe('QuestionManagerOverlay', () => {
    it('renders question group manager fields', () => {
        render(<QuestionManagerOverlay {...buildProps()} />)

        expect(screen.getByText('새 질문 그룹 이름')).toBeTruthy()
        expect(screen.getByText('그룹 태그(쉼표 구분)')).toBeTruthy()
        expect(screen.getByRole('button', {name: '그룹 생성'})).toBeTruthy()
    })

    it('hides learning mode controls', () => {
        render(<QuestionManagerOverlay {...buildProps()} />)

        expect(screen.queryByText('출제 태그 선택')).toBeNull()
        expect(screen.queryByRole('button', {name: '모드 적용'})).toBeNull()
    })
})
