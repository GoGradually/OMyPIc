/** @vitest-environment jsdom */
import {act, renderHook} from '@testing-library/react'
import {beforeEach, describe, expect, it, vi} from 'vitest'
import {useQuestionManager} from './useQuestionManager.js'
import {callApi} from '../../../shared/api/http.js'

vi.mock('../../../shared/api/http.js', () => ({
    callApi: vi.fn()
}))

function mockJsonResponse(data) {
    return {
        json: vi.fn().mockResolvedValue(data)
    }
}

describe('useQuestionManager', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('opens edit modal with selected group values', () => {
        const {result} = renderHook(() => useQuestionManager())

        act(() => {
            result.current.openEditGroupModal({id: 'g1', name: '여행/카페', tags: ['travel', 'habit']})
        })

        expect(result.current.isEditGroupModalOpen).toBe(true)
        expect(result.current.activeGroupId).toBe('g1')
        expect(result.current.editingGroupName).toBe('여행/카페')
        expect(result.current.editingGroupTagsInput).toBe('travel, habit')
    })

    it('creates group via create modal and closes modal after success', async () => {
        const refreshedGroups = [{id: 'g1', name: '여행/카페', tags: ['travel', 'habit'], questions: []}]

        callApi
            .mockResolvedValueOnce({})
            .mockResolvedValueOnce(mockJsonResponse(refreshedGroups))

        const {result} = renderHook(() => useQuestionManager())

        act(() => {
            result.current.openCreateGroupModal()
            result.current.setNewGroupName('여행/카페')
            result.current.setNewGroupTagsInput('travel, habit')
        })

        await act(async () => {
            await result.current.createGroup()
        })

        expect(callApi).toHaveBeenNthCalledWith(1, '/api/question-groups', {
            method: 'POST',
            headers: {'Content-Type': 'application/json'},
            body: JSON.stringify({
                name: '여행/카페',
                tags: ['travel', 'habit']
            })
        })
        expect(result.current.isCreateGroupModalOpen).toBe(false)
        expect(result.current.newGroupName).toBe('')
    })

    it('saves edited group via existing put API and closes edit modal', async () => {
        const updatedGroups = [{id: 'g1', name: '여행/카페 v2', tags: ['daily', 'travel'], questions: []}]

        callApi
            .mockResolvedValueOnce({})
            .mockResolvedValueOnce(mockJsonResponse(updatedGroups))

        const {result} = renderHook(() => useQuestionManager())

        act(() => {
            result.current.openEditGroupModal({id: 'g1', name: '여행/카페', tags: ['travel', 'habit']})
            result.current.setEditingGroupName('여행/카페 v2')
            result.current.setEditingGroupTagsInput('daily, travel')
        })

        await act(async () => {
            await result.current.saveEditedGroup()
        })

        expect(callApi).toHaveBeenNthCalledWith(1, '/api/question-groups/g1', {
            method: 'PUT',
            headers: {'Content-Type': 'application/json'},
            body: JSON.stringify({
                name: '여행/카페 v2',
                tags: ['daily', 'travel']
            })
        })
        expect(result.current.isEditGroupModalOpen).toBe(false)
        expect(result.current.questionGroups[0]?.name).toBe('여행/카페 v2')
    })

    it('deletes group by explicit group id', async () => {
        const refreshedGroups = [{id: 'g1', name: '여행/카페', tags: ['travel'], questions: []}]

        callApi
            .mockResolvedValueOnce({})
            .mockResolvedValueOnce(mockJsonResponse(refreshedGroups))

        const {result} = renderHook(() => useQuestionManager())

        await act(async () => {
            await result.current.deleteGroup('g2')
        })

        expect(callApi).toHaveBeenNthCalledWith(1, '/api/question-groups/g2', {method: 'DELETE'})
    })
})

describe('useQuestionManager failures', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('reports a refused question instead of failing silently', async () => {
        const onError = vi.fn()
        callApi.mockRejectedValueOnce(new Error('Question group can contain up to 3 questions'))
        const {result} = renderHook(() => useQuestionManager({onError}))
        act(() => {
            result.current.setActiveGroupId('g1')
            result.current.setNewQuestion('A fourth question?')
        })

        await act(async () => {
            await result.current.addQuestion()
        })

        expect(onError).toHaveBeenCalledWith('질문 추가에 실패했습니다: Question group can contain up to 3 questions')
        // The text stays in the box so the learner can put it somewhere else.
        expect(result.current.newQuestion).toBe('A fourth question?')
    })

    it('reports a failed group change the same way', async () => {
        const onError = vi.fn()
        callApi.mockRejectedValue(new Error('Group tags are required'))
        const {result} = renderHook(() => useQuestionManager({onError}))
        act(() => {
            result.current.openCreateGroupModal()
            result.current.setNewGroupName('여행')
        })

        await act(async () => {
            await result.current.createGroup()
        })
        await act(async () => {
            await result.current.deleteGroup('g1')
        })

        expect(onError.mock.calls.map(([message]) => message)).toEqual([
            '질문 그룹 추가에 실패했습니다: Group tags are required',
            '질문 그룹 삭제에 실패했습니다: Group tags are required'
        ])
    })
})
