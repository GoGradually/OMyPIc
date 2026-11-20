import {describe, expect, it} from 'vitest'
import {speechModelNote} from './speechModelNote.js'

const READY = {status: 'ready', progress: null}

describe('speechModelNote', () => {
    it('says nothing while both voices are ready or no session uses them', () => {
        expect(speechModelNote({english: READY, korean: READY, localSpeech: true, koreanWanted: true})).toBe('')
        expect(speechModelNote({english: {status: 'failed'}, korean: {status: 'failed'}, localSpeech: false, koreanWanted: false})).toBe('')
    })

    it('tells a Korean-feedback session that the Korean voice is being prepared, with the percentage when known', () => {
        expect(speechModelNote({english: READY, korean: {status: 'loading', progress: 42.7}, localSpeech: true, koreanWanted: true}))
            .toBe('한국어 음성 모델을 준비하고 있습니다. (42%) 처음 사용할 때는 약 263MB를 내려받습니다. 준비될 때까지 한국어 피드백은 소리 없이 화면으로만 보여 드립니다.')
        expect(speechModelNote({english: READY, korean: {status: 'loading', progress: null}, localSpeech: true, koreanWanted: true}))
            .toBe('한국어 음성 모델을 준비하고 있습니다. 처음 사용할 때는 약 263MB를 내려받습니다. 준비될 때까지 한국어 피드백은 소리 없이 화면으로만 보여 드립니다.')
    })

    it('tells a Korean-feedback session that the Korean voice could not be loaded', () => {
        expect(speechModelNote({english: READY, korean: {status: 'failed'}, localSpeech: true, koreanWanted: true}))
            .toBe('한국어 음성 모델을 불러오지 못해 한국어 피드백은 화면으로만 보여 드립니다.')
    })

    it('does not mention the Korean voice in a session that does not use it', () => {
        expect(speechModelNote({english: READY, korean: {status: 'failed'}, localSpeech: true, koreanWanted: false})).toBe('')
    })

    it('tells the session when the English voice could not be loaded or is loading again', () => {
        expect(speechModelNote({english: {status: 'failed'}, korean: READY, localSpeech: true, koreanWanted: false}))
            .toBe('음성 합성 모델을 불러오지 못해 질문을 화면으로만 보여 드립니다.')
        expect(speechModelNote({english: {status: 'loading', progress: 10}, korean: READY, localSpeech: true, koreanWanted: false}))
            .toBe('영어 음성 합성 모델을 다시 불러오고 있습니다. 그동안 질문은 화면으로만 보여 드립니다.')
    })

    it('says both when neither voice can be used', () => {
        expect(speechModelNote({english: {status: 'failed'}, korean: {status: 'failed'}, localSpeech: true, koreanWanted: true}))
            .toBe('음성 합성 모델을 불러오지 못해 질문을 화면으로만 보여 드립니다. 한국어 음성 모델을 불러오지 못해 한국어 피드백은 화면으로만 보여 드립니다.')
    })
})
