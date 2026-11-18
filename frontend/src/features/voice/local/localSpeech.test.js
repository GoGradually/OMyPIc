/** @vitest-environment jsdom */
import {afterEach, beforeEach, describe, expect, it, vi} from 'vitest'
import {needsLocalTtsModel, prepareLocalSpeech, speakLocally} from './localSpeech.js'
import {isLocalTtsSupported, synthesizeEnglish} from './localTts.js'

vi.mock('./localTts.js', () => ({
    isLocalTtsSupported: vi.fn(),
    loadLocalTts: vi.fn(),
    synthesizeEnglish: vi.fn()
}))

const ENGLISH = {name: 'Zira', lang: 'en-US', localService: true}
const KOREAN = {name: 'Heami', lang: 'ko-KR', localService: true}

let spoken = []
let audioInstances = []

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

class MockAudio {
    constructor(src) {
        this.src = src
        this.onended = null
        this.onerror = null
        audioInstances.push(this)
    }

    play() {
        return Promise.resolve()
    }

    pause() {
    }
}

async function flush() {
    for (let index = 0; index < 8; index += 1) {
        await Promise.resolve()
    }
}

// Sentences differ per test because synthesized audio is cached by sentence.
let counter = 0

function english(label) {
    counter += 1
    return `${label} number ${counter}.`
}

describe('local speech', () => {
    beforeEach(() => {
        vi.clearAllMocks()
        audioInstances = []
        global.Audio = MockAudio
        window.URL.createObjectURL = vi.fn(() => 'blob://speech')
        window.URL.revokeObjectURL = vi.fn()
        isLocalTtsSupported.mockReturnValue(true)
        synthesizeEnglish.mockImplementation(async () => ({samples: new Float32Array([0, 0.5, -0.5]), sampleRate: 24000}))
    })

    afterEach(() => {
        delete window.speechSynthesis
        delete window.SpeechSynthesisUtterance
    })

    it('reads English with the device voice when the OS has one, without touching the model', async () => {
        installSpeech([ENGLISH, KOREAN])
        const onEnd = vi.fn()
        const sentence = english('Tell me about your home')

        speakLocally(sentence, {engine: 'device', onEnd})
        await flush()
        expect(spoken[0].text).toBe(sentence)
        expect(spoken[0].voice.name).toBe('Zira')
        spoken[0].onend()
        await flush()

        expect(onEnd).toHaveBeenCalledTimes(1)
        expect(synthesizeEnglish).not.toHaveBeenCalled()
        expect(await needsLocalTtsModel('device')).toBe(false)
    })

    it('reads English with the model, not the Korean voice, when the OS has no English voice', async () => {
        installSpeech([KOREAN])
        const onEnd = vi.fn()
        const onNotice = vi.fn()
        const sentence = english('Describe your favorite park')

        speakLocally(sentence, {engine: 'device', onEnd, onNotice})
        await flush()

        expect(synthesizeEnglish).toHaveBeenCalledWith(sentence)
        expect(window.speechSynthesis.speak).not.toHaveBeenCalled()
        expect(onNotice).toHaveBeenCalledWith('no-english-voice')
        expect(onNotice).toHaveBeenCalledWith('preparing')
        expect(audioInstances.length).toBe(1)
        audioInstances[0].onended()
        await flush()

        expect(onEnd).toHaveBeenCalledTimes(1)
        expect(await needsLocalTtsModel('device')).toBe(true)
    })

    it('uses the model for English and the device voice for Korean when Kokoro is selected', async () => {
        installSpeech([ENGLISH, KOREAN])
        const onEnd = vi.fn()
        const sentence = english('You went there yesterday')

        speakLocally(`${sentence} 과거 시제를 써야 합니다.`, {engine: 'kokoro', onEnd})
        await flush()
        expect(synthesizeEnglish).toHaveBeenCalledWith(sentence)
        audioInstances[0].onended()
        await flush()
        expect(spoken[0].text).toBe('과거 시제를 써야 합니다.')
        expect(spoken[0].voice.name).toBe('Heami')
        spoken[0].onend()
        await flush()

        expect(onEnd).toHaveBeenCalledTimes(1)
        expect(await needsLocalTtsModel('kokoro')).toBe(true)
    })

    it('synthesizes every sentence before the first one plays', async () => {
        installSpeech([KOREAN])
        const first = english('First question')
        const second = english('Second question')
        let release
        synthesizeEnglish.mockImplementation((text) => (
            text === second
                ? new Promise((resolve) => {
                    release = () => resolve({samples: new Float32Array([0]), sampleRate: 24000})
                })
                : Promise.resolve({samples: new Float32Array([0]), sampleRate: 24000})
        ))

        speakLocally(`${first} ${second}`, {engine: 'kokoro'})
        await flush()
        expect(audioInstances.length).toBe(0)
        release()
        await flush()

        expect(audioInstances.length).toBe(1)
    })

    it('reuses audio that was prepared ahead of time', async () => {
        installSpeech([KOREAN])
        const sentence = english('Tell me about your routine')

        await prepareLocalSpeech(sentence, 'kokoro')
        await flush()
        speakLocally(sentence, {engine: 'kokoro'})
        await flush()

        expect(synthesizeEnglish).toHaveBeenCalledTimes(1)
        expect(audioInstances.length).toBe(1)
    })

    it('reports that nothing could be spoken when there is no voice and no model', async () => {
        installSpeech([])
        isLocalTtsSupported.mockReturnValue(false)
        const onEnd = vi.fn()
        const onUnavailable = vi.fn()

        speakLocally(english('Hello there'), {engine: 'device', onEnd, onUnavailable})
        await vi.waitFor(() => {
            expect(onUnavailable).toHaveBeenCalledTimes(1)
        }, {timeout: 3000})

        expect(onEnd).not.toHaveBeenCalled()
        expect(await needsLocalTtsModel('device')).toBe(false)
    })

    it('goes on without the voice when synthesis fails', async () => {
        installSpeech([KOREAN])
        synthesizeEnglish.mockRejectedValue(new Error('out of memory'))
        const onUnavailable = vi.fn()

        speakLocally(english('Hello there'), {engine: 'kokoro', onUnavailable})
        await flush()
        await flush()

        expect(onUnavailable).toHaveBeenCalledTimes(1)
        expect(audioInstances.length).toBe(0)
    })

    it('stays silent after cancel', async () => {
        installSpeech([ENGLISH])
        const onEnd = vi.fn()
        const onUnavailable = vi.fn()

        const cancel = speakLocally(`${english('First')} ${english('Second')}`, {engine: 'device', onEnd, onUnavailable})
        await flush()
        cancel()
        await flush()

        expect(spoken.length).toBe(1)
        expect(window.speechSynthesis.cancel).toHaveBeenCalled()
        expect(onEnd).not.toHaveBeenCalled()
        expect(onUnavailable).not.toHaveBeenCalled()
    })
})
