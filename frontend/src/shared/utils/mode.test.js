import {describe, expect, it} from 'vitest'
import {buildModePayload, getCurrentQuestionLabel, parseMockGroupCounts} from './mode.js'

describe('mode utils', () => {
    it('parses valid mock counts JSON', () => {
        expect(parseMockGroupCounts('{"A":2,"B":3}')).toEqual({A: 2, B: 3})
    })

    it('falls back to empty object on invalid JSON', () => {
        expect(parseMockGroupCounts('{invalid}')).toEqual({})
    })

    it('builds mode payload with normalized order', () => {
        const payload = buildModePayload({
            sessionId: 'session-1',
            listId: 'list-1',
            mode: 'MOCK_EXAM',
            batchSize: 3,
            mockOrder: ' A, B , , C',
            mockCounts: '{"A":2}'
        })

        expect(payload).toEqual({
            sessionId: 'session-1',
            listId: 'list-1',
            mode: 'MOCK_EXAM',
            continuousBatchSize: 3,
            mockGroupOrder: ['A', 'B', 'C'],
            mockGroupCounts: {A: 2}
        })
    })

    it('builds default question label for empty question', () => {
        expect(getCurrentQuestionLabel(null)).toBe('세션 시작을 누르면 첫 질문이 자동으로 제시됩니다.')
    })

    it('returns completion question label in mock exam', () => {
        expect(getCurrentQuestionLabel({mockExamCompleted: true})).toBe('모든 모의고사 질문을 완료했습니다.')
    })
})
