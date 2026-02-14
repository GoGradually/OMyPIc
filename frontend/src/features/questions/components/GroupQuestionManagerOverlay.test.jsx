/** @vitest-environment jsdom */
import React from 'react'
import {afterEach, describe, expect, it, vi} from 'vitest'
import {cleanup, render, screen} from '@testing-library/react'
import {GroupQuestionManagerOverlay} from './GroupQuestionManagerOverlay.jsx'

afterEach(() => {
    cleanup()
})

function buildProps(overrides = {}) {
    return {
        activeGroupId: '',
        questionGroups: [],
        setActiveGroupId: vi.fn(),
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

describe('GroupQuestionManagerOverlay', () => {
    it('renders question item controls', () => {
        render(<GroupQuestionManagerOverlay {...buildProps()} />)

        expect(screen.getByText('질문 그룹 선택')).toBeTruthy()
        expect(screen.getByRole('button', {name: '질문 추가'})).toBeTruthy()
        expect(screen.getByText('질문 타입(선택)')).toBeTruthy()
    })

    it('does not render question group creation controls', () => {
        render(<GroupQuestionManagerOverlay {...buildProps()} />)

        expect(screen.queryByText('새 질문 그룹 이름')).toBeNull()
        expect(screen.queryByText('그룹 태그(쉼표 구분)')).toBeNull()
        expect(screen.queryByRole('button', {name: '그룹 생성'})).toBeNull()
        expect(screen.queryByRole('button', {name: '그룹 삭제'})).toBeNull()
    })
})
