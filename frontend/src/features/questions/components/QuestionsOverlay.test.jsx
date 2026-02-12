/** @vitest-environment jsdom */
import React from 'react'
import {afterEach, describe, expect, it, vi} from 'vitest'
import {cleanup, render, screen} from '@testing-library/react'
import {QuestionsOverlay} from './QuestionsOverlay.jsx'

afterEach(() => {
    cleanup()
})

function buildProps(overrides = {}) {
    return {
        mode: 'IMMEDIATE',
        setMode: vi.fn(),
        batchSize: 3,
        setBatchSize: vi.fn(),
        updateMode: vi.fn(),
        nextQuestion: vi.fn(),
        tagStats: [],
        selectedGroupTags: [],
        toggleSelectedTag: vi.fn(),
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

describe('QuestionsOverlay', () => {
    it('removes legacy list selection UI labels', () => {
        render(<QuestionsOverlay {...buildProps()} />)

        expect(screen.queryByText('리스트 선택')).toBeNull()
        expect(screen.queryByText('새 리스트 이름')).toBeNull()
    })

    it('renders tag buttons from tag stats', () => {
        render(
            <QuestionsOverlay
                {...buildProps({
                    tagStats: [
                        {tag: 'travel', groupCount: 2, selectable: true},
                        {tag: 'habit', groupCount: 1, selectable: true}
                    ]
                })}
            />
        )

        expect(screen.getByRole('button', {name: 'travel (2)'})).toBeTruthy()
        expect(screen.getByRole('button', {name: 'habit (1)'})).toBeTruthy()
    })

    it('disables non-selectable tags', () => {
        render(
            <QuestionsOverlay
                {...buildProps({
                    tagStats: [
                        {tag: 'travel', groupCount: 2, selectable: true},
                        {tag: 'legacy', groupCount: 0, selectable: false}
                    ]
                })}
            />
        )

        const enabled = screen.getByRole('button', {name: 'travel (2)'})
        const disabled = screen.getByRole('button', {name: 'legacy (0)'})
        expect(enabled.disabled).toBe(false)
        expect(disabled.disabled).toBe(true)
    })
})
