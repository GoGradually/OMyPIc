import {describe, expect, it} from 'vitest'
import {buildFeedbackFromVoice, parseVoiceEnvelope} from './voiceEvent.js'

describe('voiceEvent', () => {
    it('parses voice envelope safely', () => {
        const parsed = parseVoiceEnvelope('{"type":"stt.partial","data":{"text":"hello"}}')

        expect(parsed).toEqual({
            type: 'stt.partial',
            data: {text: 'hello'}
        })
    })

    it('builds feedback model with defaults', () => {
        const feedback = buildFeedbackFromVoice({
            policy: {
                mode: 'CONTINUOUS',
                reason: 'BATCH_READY',
                groupBatchSize: 2
            },
            batch: {
                size: 1,
                isResidual: false,
                items: [
                    {
                        questionId: 'q1',
                        questionText: '질문',
                        questionGroup: 'A',
                        answerText: '답변',
                        summary: '요약',
                        correctionPoints: ['a', 'b'],
                        recommendation: ['c']
                    }
                ]
            },
            nextAction: {
                type: 'ask_next',
                reason: ''
            }
        })

        expect(feedback).toEqual({
            mode: 'CONTINUOUS',
            batch: {
                size: 1,
                isResidual: false,
                reason: 'BATCH_READY',
                groupBatchSize: 2
            },
            nextAction: {
                type: 'ask_next',
                reason: ''
            },
            items: [
                {
                    questionId: 'q1',
                    questionText: '질문',
                    questionGroup: 'A',
                    answerText: '답변',
                    summary: '요약',
                    correctionPoints: ['a', 'b'],
                    recommendation: ['c'],
                    exampleAnswer: '',
                    rulebookEvidence: []
                }
            ],
            summary: '요약',
            correctionPoints: ['a', 'b'],
            recommendation: ['c'],
            exampleAnswer: '',
            rulebookEvidence: []
        })
    })
})
