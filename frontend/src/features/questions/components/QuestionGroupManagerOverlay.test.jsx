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
        questionGroups: [],
        setActiveGroupId: vi.fn(),
        isCreateGroupModalOpen: false,
        openCreateGroupModal: vi.fn(),
        closeCreateGroupModal: vi.fn(),
        newGroupName: '',
        setNewGroupName: vi.fn(),
        newGroupTagsInput: '',
        setNewGroupTagsInput: vi.fn(),
        createGroup: vi.fn(),
        isEditGroupModalOpen: false,
        openEditGroupModal: vi.fn(),
        closeEditGroupModal: vi.fn(),
        editingGroupName: '',
        setEditingGroupName: vi.fn(),
        editingGroupTagsInput: '',
        setEditingGroupTagsInput: vi.fn(),
        deleteGroup: vi.fn(),
        saveEditedGroup: vi.fn(),
        ...overrides
    }
}

describe('QuestionGroupManagerOverlay', () => {
    it('renders question group manager controls', () => {
        render(<QuestionGroupManagerOverlay {...buildProps({
            questionGroups: [{id: 'g1', name: '여행/카페', tags: ['travel', 'habit']}]
        })} />)

        expect(screen.getByText('그룹 목록')).toBeTruthy()
        expect(screen.getByRole('button', {name: '그룹 생성'})).toBeTruthy()
        expect(screen.getByText('여행/카페')).toBeTruthy()
        expect(screen.getByRole('button', {name: '수정'})).toBeTruthy()
        expect(screen.getByRole('button', {name: '삭제'})).toBeTruthy()
    })

    it('does not render question item controls', () => {
        render(<QuestionGroupManagerOverlay {...buildProps()} />)

        expect(screen.queryByText('질문 추가')).toBeNull()
        expect(screen.queryByRole('button', {name: '저장'})).toBeNull()
        expect(screen.queryByRole('button', {name: '수정'})).toBeNull()
    })

    it('renders create modal when opened', () => {
        render(<QuestionGroupManagerOverlay {...buildProps({isCreateGroupModalOpen: true})} />)

        expect(screen.getByRole('dialog', {name: '질문 그룹 생성'})).toBeTruthy()
        expect(screen.getAllByText('그룹 태그(쉼표 구분)').length).toBe(1)
        expect(screen.getByRole('button', {name: '생성'})).toBeTruthy()
        expect(screen.getByRole('button', {name: '취소'})).toBeTruthy()
    })

    it('renders edit modal when opened', () => {
        render(<QuestionGroupManagerOverlay {...buildProps({isEditGroupModalOpen: true})} />)

        expect(screen.getByRole('dialog', {name: '질문 그룹 수정'})).toBeTruthy()
        expect(screen.getAllByText('그룹 태그(쉼표 구분)').length).toBe(1)
        expect(screen.getByRole('button', {name: '저장'})).toBeTruthy()
        expect(screen.getByRole('button', {name: '취소'})).toBeTruthy()
    })
})
