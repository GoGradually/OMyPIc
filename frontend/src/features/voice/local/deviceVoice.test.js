/** @vitest-environment jsdom */
import {afterEach, beforeEach, describe, expect, it, vi} from 'vitest'
import {detectSpeechLang, pickVoice, speakWithDeviceVoice, splitForSpeech} from './deviceVoice.js'

const VOICES = [
    {name: 'Remote English', lang: 'en-GB', localService: false},
    {name: 'Local English', lang: 'en-US', localService: true},
    {name: 'Local Korean', lang: 'ko-KR', localService: true}
]

let spoken = []

function installSpeech(voices) {
    spoken = []
    window.SpeechSynthesisUtterance = function Utterance(text) {
        this.text = text
    }
    window.speechSynthesis = {
        getVoices: () => voices,
        speak: vi.fn((utterance) => {
            spoken.push(utterance)
        }),
        cancel: vi.fn(),
        addEventListener: vi.fn(),
        removeEventListener: vi.fn()
    }
}

describe('device voice', () => {
    beforeEach(() => {
        installSpeech(VOICES)
    })

    afterEach(() => {
        delete window.speechSynthesis
        delete window.SpeechSynthesisUtterance
        vi.useRealTimers()
    })

    it('splits text into sentences and breaks very long ones', () => {
        expect(splitForSpeech('Tell me about your home. What does it look like?\nThank you!'))
            .toEqual(['Tell me about your home.', 'What does it look like?', 'Thank you!'])
        const long = Array.from({length: 80}, () => 'word').join(' ')
        expect(splitForSpeech(long).every((chunk) => chunk.length <= 220)).toBe(true)
        expect(splitForSpeech('   ')).toEqual([])
    })

    it('chooses a voice that matches the language of the sentence', () => {
        expect(detectSpeechLang('Tell me about your home.')).toBe('en-US')
        expect(detectSpeechLang('과거 시제를 써야 합니다.')).toBe('ko-KR')
        expect(pickVoice(VOICES, 'en-US').name).toBe('Local English')
        expect(pickVoice(VOICES, 'ko-KR').name).toBe('Local Korean')
        expect(pickVoice(VOICES, 'ja-JP')).toBeNull()
    })

    it('speaks sentence by sentence and reports the end once', async () => {
        const onEnd = vi.fn()
        speakWithDeviceVoice('First sentence. 두 번째 문장입니다.', {onEnd})
        await Promise.resolve()

        expect(spoken.length).toBe(1)
        expect(spoken[0].text).toBe('First sentence.')
        expect(spoken[0].voice.name).toBe('Local English')
        spoken[0].onend()
        expect(spoken[1].text).toBe('두 번째 문장입니다.')
        expect(spoken[1].voice.name).toBe('Local Korean')
        expect(onEnd).not.toHaveBeenCalled()
        spoken[1].onend()
        expect(onEnd).toHaveBeenCalledTimes(1)
    })

    it('reports that the device cannot speak when it has no voices', async () => {
        vi.useFakeTimers()
        installSpeech([])
        const onEnd = vi.fn()
        const onUnavailable = vi.fn()

        speakWithDeviceVoice('Hello there.', {onEnd, onUnavailable})
        await vi.advanceTimersByTimeAsync(2000)

        expect(onUnavailable).toHaveBeenCalledTimes(1)
        expect(onEnd).not.toHaveBeenCalled()
        expect(window.speechSynthesis.speak).not.toHaveBeenCalled()
    })

    it('reports that the device cannot speak when there is no speech engine', async () => {
        delete window.speechSynthesis
        const onUnavailable = vi.fn()

        speakWithDeviceVoice('Hello there.', {onUnavailable})
        await Promise.resolve()

        expect(onUnavailable).toHaveBeenCalledTimes(1)
    })

    it('gives up when the engine never starts speaking', async () => {
        vi.useFakeTimers()
        const onUnavailable = vi.fn()

        speakWithDeviceVoice('Hello there.', {onUnavailable})
        await vi.advanceTimersByTimeAsync(0)
        expect(spoken.length).toBe(1)
        await vi.advanceTimersByTimeAsync(5000)

        expect(onUnavailable).toHaveBeenCalledTimes(1)
    })

    it('stays silent after cancel', async () => {
        const onEnd = vi.fn()
        const onUnavailable = vi.fn()

        const cancel = speakWithDeviceVoice('First sentence. Second sentence.', {onEnd, onUnavailable})
        await Promise.resolve()
        cancel()
        spoken[0].onend()

        expect(window.speechSynthesis.cancel).toHaveBeenCalled()
        expect(spoken.length).toBe(1)
        expect(onEnd).not.toHaveBeenCalled()
        expect(onUnavailable).not.toHaveBeenCalled()
    })
})
