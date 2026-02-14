/** @vitest-environment jsdom */
import React from 'react'
import {afterEach, describe, expect, it, vi} from 'vitest'
import {cleanup, render, screen} from '@testing-library/react'
import {QuestionGroupManagerOverlay} from './QuestionGroupManagerOverlay.jsx'

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
        ...overrides
    }
}

describe('QuestionGroupManagerOverlay', () => {
    it('renders question group manager controls', () => {
        render(<QuestionGroupManagerOverlay {...buildProps()} />)

        expect(screen.getByText('새 질문 그룹 이름')).toBeTruthy()
        expect(screen.getByText('그룹 태그(쉼표 구분)')).toBeTruthy()
        expect(screen.getByRole('button', {name: '그룹 생성'})).toBeTruthy()
        expect(screen.getByRole('button', {name: '그룹 삭제'})).toBeTruthy()
    })

    it('does not render question item controls', () => {
        render(<QuestionGroupManagerOverlay {...buildProps()} />)

        expect(screen.queryByText('질문 추가')).toBeNull()
        expect(screen.queryByRole('button', {name: '저장'})).toBeNull()
        expect(screen.queryByRole('button', {name: '수정'})).toBeNull()
    })
})
