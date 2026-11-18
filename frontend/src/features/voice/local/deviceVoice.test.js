/** @vitest-environment jsdom */
import {afterEach, beforeEach, describe, expect, it, vi} from 'vitest'
import {
    detectSpeechLang,
    loadDeviceVoices,
    pickVoice,
    speakSentence,
    splitForSpeech
} from './deviceVoice.js'

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

    it('detects the language of a sentence', () => {
        expect(detectSpeechLang('Tell me about your home.')).toBe('en-US')
        expect(detectSpeechLang('과거 시제를 써야 합니다.')).toBe('ko-KR')
    })

    it('chooses a voice of the same language', () => {
        expect(pickVoice(VOICES, 'en-US').name).toBe('Local English')
        expect(pickVoice(VOICES, 'ko-KR').name).toBe('Local Korean')
        expect(pickVoice([{name: 'UK', lang: 'en_GB', localService: true}], 'en-US').name).toBe('UK')
    })

    it('never hands an English sentence to a voice of another language', () => {
        const koreanOnly = [{name: 'Heami', lang: 'ko-KR', localService: true}]

        expect(pickVoice(koreanOnly, 'en-US')).toBeNull()
        expect(pickVoice(koreanOnly, 'ko-KR').name).toBe('Heami')
    })

    it('lists no voices when the device has no speech engine', async () => {
        delete window.speechSynthesis

        expect(await loadDeviceVoices()).toEqual([])
    })

    it('waits for voices that load late', async () => {
        vi.useFakeTimers()
        let voices = []
        installSpeech(voices)
        window.speechSynthesis.getVoices = () => voices

        const pending = loadDeviceVoices()
        voices = VOICES
        await vi.advanceTimersByTimeAsync(1500)

        expect(await pending).toEqual(VOICES)
    })

    it('speaks a sentence with the chosen voice and reports that it was heard', async () => {
        const speech = speakSentence('First sentence.', VOICES[1])

        expect(spoken[0].text).toBe('First sentence.')
        expect(spoken[0].voice.name).toBe('Local English')
        expect(spoken[0].lang).toBe('en-US')
        spoken[0].onend()

        expect(await speech.done).toBe(true)
    })

    it('reports failure when the engine never starts speaking', async () => {
        vi.useFakeTimers()
        const speech = speakSentence('Hello there.', VOICES[1])

        await vi.advanceTimersByTimeAsync(5000)

        expect(await speech.done).toBe(false)
        expect(window.speechSynthesis.cancel).toHaveBeenCalled()
    })

    it('stops when cancelled', async () => {
        const speech = speakSentence('Hello there.', VOICES[1])

        speech.cancel()

        expect(await speech.done).toBe(false)
        expect(window.speechSynthesis.cancel).toHaveBeenCalled()
    })
})
