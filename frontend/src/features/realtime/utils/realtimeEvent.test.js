import {describe, expect, it} from 'vitest'
import {buildFeedbackFromRealtime, parseRealtimeEnvelope} from './realtimeEvent.js'

describe('realtimeEvent', () => {
    it('parses realtime envelope safely', () => {
        const parsed = parseRealtimeEnvelope('{"type":"stt.partial","data":{"text":"hello"}}')

        expect(parsed).toEqual({
            type: 'stt.partial',
            data: {text: 'hello'}
        })
    })

    it('builds feedback model with defaults', () => {
        const feedback = buildFeedbackFromRealtime({
            summary: '요약',
            correctionPoints: ['a', 'b']
        })

        expect(feedback).toEqual({
            summary: '요약',
            correctionPoints: ['a', 'b'],
            exampleAnswer: '',
            rulebookEvidence: []
        })
    })
})
