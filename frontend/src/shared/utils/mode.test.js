import {describe, expect, it} from 'vitest'
import {buildModePayload, getCurrentQuestionLabel} from './mode.js'

describe('mode utils', () => {
    it('builds mode payload without mock plan', () => {
        const payload = buildModePayload({
            sessionId: 'session-1',
            mode: 'CONTINUOUS',
            batchSize: 3,
            selectedGroupTags: ['travel', 'habit']
        })

        expect(payload).toEqual({
            sessionId: 'session-1',
            mode: 'CONTINUOUS',
            continuousBatchSize: 3,
            selectedGroupTags: ['travel', 'habit']
        })
    })

    it('builds default question label for empty question', () => {
        expect(getCurrentQuestionLabel(null)).toBe('세션 시작을 누르면 첫 질문이 자동으로 제시됩니다.')
    })

    it('returns completion label when questions are exhausted', () => {
        expect(getCurrentQuestionLabel({exhausted: true})).toBe('모든 질문을 완료했습니다.')
    })
})
