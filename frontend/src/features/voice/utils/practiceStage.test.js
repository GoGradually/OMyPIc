import {describe, expect, it} from 'vitest'
import {getActiveStepIndex, getPracticeSteps, getStageCaption} from './practiceStage.js'

describe('practice stage', () => {
    it('ends the turn with feedback when AI is on in immediate mode', () => {
        expect(getPracticeSteps({aiEnabled: true, autoReplayEnabled: true, continuousMode: false}))
            .toEqual(['질문 듣기', '답변', '피드백'])
    })

    it('has no per-turn feedback step in continuous mode', () => {
        expect(getPracticeSteps({aiEnabled: true, autoReplayEnabled: false, continuousMode: true}))
            .toEqual(['질문 듣기', '답변'])
    })

    it('ends the turn with answer replay when AI is off', () => {
        expect(getPracticeSteps({aiEnabled: false, autoReplayEnabled: true, continuousMode: false}))
            .toEqual(['질문 듣기', '답변', '내 응답 다시듣기'])
        expect(getPracticeSteps({aiEnabled: false, autoReplayEnabled: false, continuousMode: true}))
            .toEqual(['질문 듣기', '답변'])
    })

    it('marks no step while the session is idle', () => {
        expect(getActiveStepIndex({sessionActive: false, speechState: 'IDLE', stepCount: 3})).toBe(-1)
    })

    it('follows the speech state through a turn', () => {
        const base = {sessionActive: true, replayPlaying: false, awaitingReview: false, stepCount: 3}
        expect(getActiveStepIndex({...base, speechState: 'PLAYING_QUESTION_TTS'})).toBe(0)
        expect(getActiveStepIndex({...base, speechState: 'READY_FOR_ANSWER'})).toBe(1)
        expect(getActiveStepIndex({...base, speechState: 'CAPTURING_ANSWER'})).toBe(1)
        expect(getActiveStepIndex({...base, speechState: 'PLAYING_FEEDBACK_TTS'})).toBe(2)
        expect(getActiveStepIndex({...base, speechState: 'WAITING_TTS', awaitingReview: true})).toBe(2)
        expect(getActiveStepIndex({...base, speechState: 'WAITING_TTS'})).toBe(0)
    })

    it('shows the last step while the answer replays', () => {
        expect(getActiveStepIndex({
            sessionActive: true,
            speechState: 'WAITING_TTS',
            replayPlaying: true,
            awaitingReview: true,
            stepCount: 3
        })).toBe(2)
    })

    it('goes back to the first step after an answer when the turn has only two steps', () => {
        expect(getActiveStepIndex({
            sessionActive: true,
            speechState: 'WAITING_TTS',
            replayPlaying: false,
            awaitingReview: true,
            stepCount: 2
        })).toBe(0)
    })

    it('explains what the session will do before it starts', () => {
        expect(getStageCaption({sessionActive: false, aiEnabled: false, autoReplayEnabled: true}))
            .toContain('AI 없이')
        expect(getStageCaption({sessionActive: false, aiEnabled: true, continuousMode: true}))
            .toContain('질문 그룹')
    })

    it('says the answer is only recorded when AI is off', () => {
        const active = {sessionActive: true, aiEnabled: false}
        expect(getStageCaption({...active, speechState: 'READY_FOR_ANSWER'})).toBe('지금 답변해 주세요.')
        expect(getStageCaption({...active, speechState: 'CAPTURING_ANSWER'})).toContain('녹음하고 있습니다')
    })

    it('only promises AI review when AI is on', () => {
        const waiting = {sessionActive: true, speechState: 'WAITING_TTS', awaitingReview: true}
        expect(getStageCaption({...waiting, aiEnabled: true})).toBe('AI가 답변을 첨삭하고 있습니다.')
        expect(getStageCaption({...waiting, aiEnabled: false})).toBe('다음 질문을 준비하고 있습니다.')
    })
})
