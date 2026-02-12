/** @vitest-environment jsdom */
import {beforeEach, describe, expect, it, vi} from 'vitest'
import {renderHook} from '@testing-library/react'
import {useQuestionGroups} from './useQuestionGroups.js'
import {callApi} from '../../../shared/api/http.js'

vi.mock('../../../shared/api/http.js', () => ({
    callApi: vi.fn()
}))

describe('useQuestionGroups', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('blocks mode update request when no selected tags and surfaces error message', async () => {
        const onStatus = vi.fn()
        const {result} = renderHook(() => useQuestionGroups({sessionId: 's1', onStatus}))

        await expect(result.current.updateMode())
            .rejects
            .toThrow('질문 그룹 태그를 하나 이상 선택해 주세요.')

        expect(callApi).not.toHaveBeenCalled()
        expect(onStatus).not.toHaveBeenCalled()
    })
})
