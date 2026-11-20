/** @vitest-environment jsdom */
import {afterEach, beforeEach, describe, expect, it, vi} from 'vitest'
import {neededLocalTtsModels, prepareLocalSpeech, speakLocally} from './localSpeech.js'
import {isLocalTtsSupported, synthesizeEnglish} from './localTts.js'
import {getLocalKoTtsStatus, isLocalKoTtsSupported, loadLocalKoTts, synthesizeKorean} from './localKoTts.js'

vi.mock('./localTts.js', () => ({
    isLocalTtsSupported: vi.fn(),
    loadLocalTts: vi.fn(),
    synthesizeEnglish: vi.fn()
}))

vi.mock('../../../shared/api/http.js', () => ({
    getBackendUrl: vi.fn(async () => 'http://localhost:4317')
}))

vi.mock('./localKoTts.js', () => ({
    isLocalKoTtsSupported: vi.fn(),
    getLocalKoTtsStatus: vi.fn(),
    loadLocalKoTts: vi.fn(),
    synthesizeKorean: vi.fn()
}))

let audioInstances = []

class MockAudio {
    constructor(src) {
        this.src = src
        this.onended = null
        this.onerror = null
        this.paused = false
        audioInstances.push(this)
    }

    play() {
        return Promise.resolve()
    }

    pause() {
        this.paused = true
    }
}

async function flush() {
    for (let index = 0; index < 8; index += 1) {
        await Promise.resolve()
    }
}

// Plays every queued audio element to its end, one at a time, and returns how many played.
async function playAll() {
    let played = 0
    while (played < audioInstances.length) {
        audioInstances[played].onended()
        played += 1
        await flush()
    }
    return played
}

// Sentences differ per test because synthesized audio is cached by sentence.
let counter = 0

function english(label) {
    counter += 1
    return `${label} number ${counter}.`
}

function korean(label) {
    counter += 1
    return `${label} ${counter}번입니다.`
}

// Which engine made each played audio element: synthesizers tag their samples.
const ENGLISH_RATE = 24000
const KOREAN_RATE = 44100
let playedWavs = []

describe('local speech', () => {
    beforeEach(() => {
        vi.clearAllMocks()
        audioInstances = []
        playedWavs = []
        global.Audio = MockAudio
        window.URL.createObjectURL = vi.fn((blob) => {
            playedWavs.push(blob)
            return 'blob://speech'
        })
        window.URL.revokeObjectURL = vi.fn()
        isLocalTtsSupported.mockReturnValue(true)
        isLocalKoTtsSupported.mockReturnValue(true)
        getLocalKoTtsStatus.mockReturnValue('ready')
        loadLocalKoTts.mockResolvedValue(undefined)
        synthesizeEnglish.mockImplementation(async () => ({samples: new Float32Array([0, 0.5, -0.5]), sampleRate: ENGLISH_RATE}))
        synthesizeKorean.mockImplementation(async () => ({samples: new Float32Array([0, 0.25, -0.25]), sampleRate: KOREAN_RATE}))
    })

    afterEach(() => {
        delete window.speechSynthesis
    })

    it('English sentence goes to Kokoro', async () => {
        const onEnd = vi.fn()
        const sentence = english('Tell me about your home')

        speakLocally(sentence, {onEnd})
        await flush()
        expect(audioInstances.length).toBe(1)
        await playAll()

        expect(synthesizeEnglish).toHaveBeenCalledWith(sentence)
        expect(synthesizeKorean).not.toHaveBeenCalled()
        expect(onEnd).toHaveBeenCalledTimes(1)
    })

    it('Korean sentence goes to the Korean model', async () => {
        const onEnd = vi.fn()
        const sentence = korean('과거 시제를 써야 합니다')

        speakLocally(sentence, {onEnd})
        await flush()
        await playAll()

        expect(synthesizeKorean).toHaveBeenCalledWith(sentence)
        expect(synthesizeEnglish).not.toHaveBeenCalled()
        expect(onEnd).toHaveBeenCalledTimes(1)
    })

    it('reads a Korean sentence that quotes English whole with the Korean model', async () => {
        const sentence = korean('"I goed" 대신 "I went"라고 해야 합니다')

        speakLocally(sentence, {})
        await flush()

        expect(synthesizeKorean).toHaveBeenCalledWith(sentence)
        expect(synthesizeEnglish).not.toHaveBeenCalled()
    })

    it('mixed text routes each sentence by its own language', async () => {
        const first = english('You went there yesterday')
        const second = korean('과거 시제를 잘 썼습니다')
        const third = english('Tell me more')
        const fourth = korean('예시를 더 들어 보세요')
        const onEnd = vi.fn()

        speakLocally(`${first} ${second} ${third} ${fourth}`, {onEnd})
        await flush()

        expect(synthesizeEnglish.mock.calls.map(([text]) => text)).toEqual([first, third])
        expect(synthesizeKorean.mock.calls.map(([text]) => text)).toEqual([second, fourth])
        expect(await playAll()).toBe(4)
        expect(onEnd).toHaveBeenCalledTimes(1)
    })

    it('plays in sentence order even when a later sentence is synthesized first', async () => {
        const first = korean('먼저 읽을 문장')
        const second = english('Read me second')
        let releaseKorean
        synthesizeKorean.mockImplementation(() => new Promise((resolve) => {
            releaseKorean = () => resolve({samples: new Float32Array([0.25]), sampleRate: KOREAN_RATE})
        }))
        const order = []
        const originalCreate = window.URL.createObjectURL
        window.URL.createObjectURL = vi.fn((blob) => {
            originalCreate(blob)
            return `blob://${audioInstances.length}`
        })

        speakLocally(`${first} ${second}`, {})
        await flush()
        // The English sentence is ready, but it waits for the Korean one ahead of it.
        expect(audioInstances.length).toBe(0)
        releaseKorean()
        await flush()
        order.push(audioInstances.length)
        audioInstances[0].onended()
        await flush()
        order.push(audioInstances.length)

        expect(order).toEqual([1, 2])
        // The first element played is the Korean audio (44.1 kHz WAV header).
        const firstRate = new DataView(await playedWavs[0].arrayBuffer()).getUint32(24, true)
        const secondRate = new DataView(await playedWavs[1].arrayBuffer()).getUint32(24, true)
        expect([firstRate, secondRate]).toEqual([KOREAN_RATE, ENGLISH_RATE])
    })

    it('Korean sentence is skipped silently when the Korean model fails to load and English still plays', async () => {
        getLocalKoTtsStatus.mockReturnValue('failed')
        const onEnd = vi.fn()
        const onUnavailable = vi.fn()
        const onNotice = vi.fn()
        const first = english('Good answer')
        const second = korean('다음에는 예시를 들어 보세요')

        speakLocally(`${first} ${second}`, {onEnd, onUnavailable, onNotice})
        await flush()

        expect(synthesizeKorean).not.toHaveBeenCalled()
        expect(await playAll()).toBe(1)
        expect(onNotice).toHaveBeenCalledWith('korean-unavailable')
        expect(onEnd).toHaveBeenCalledTimes(1)
        expect(onUnavailable).not.toHaveBeenCalled()
    })

    it('starts loading the Korean model in the background when a Korean sentence arrives before it was prepared', async () => {
        getLocalKoTtsStatus.mockReturnValue('idle')
        let finishLoad
        loadLocalKoTts.mockImplementation(() => new Promise((resolve) => {
            finishLoad = resolve
        }))
        const onEnd = vi.fn()
        const sentence = korean('피드백 문장')

        speakLocally(sentence, {onEnd})
        await flush()

        // The sentence is shown without sound instead of waiting for the download.
        expect(loadLocalKoTts).toHaveBeenCalledTimes(1)
        expect(synthesizeKorean).not.toHaveBeenCalled()
        expect(audioInstances.length).toBe(0)
        expect(onEnd).toHaveBeenCalledTimes(1)

        // Once the model is ready, later Korean sentences are read.
        finishLoad()
        getLocalKoTtsStatus.mockReturnValue('ready')
        speakLocally(korean('다음 피드백'), {})
        await flush()
        expect(synthesizeKorean).toHaveBeenCalledTimes(1)
        expect(loadLocalKoTts).toHaveBeenCalledTimes(1)
    })

    it('after the Korean worker crashes, a Korean sentence is skipped silently, English plays and the model loads again in the background', async () => {
        // The real Korean client, with a worker that loads, crashes, and then takes its time to reload.
        const actual = await vi.importActual('./localKoTts.js')
        const loads = []
        let crashed
        vi.stubGlobal('Worker', class {
            constructor() {
                this.answersLoad = !crashed
            }

            postMessage(message) {
                if (message.type === 'load') {
                    loads.push(message)
                }
                if (this.answersLoad) {
                    queueMicrotask(() => this.onmessage({data: {id: message.id, ok: true}}))
                }
                crashed = this
            }

            terminate() {}
        })
        isLocalKoTtsSupported.mockImplementation(actual.isLocalKoTtsSupported)
        getLocalKoTtsStatus.mockImplementation(actual.getLocalKoTtsStatus)
        loadLocalKoTts.mockImplementation(actual.loadLocalKoTts)
        synthesizeKorean.mockImplementation(actual.synthesizeKorean)
        try {
            await actual.loadLocalKoTts()
            crashed.onerror({message: 'out of memory'})
            const onEnd = vi.fn()
            const first = english('Nice work')
            const second = korean('다음에는 이유를 말해 보세요')

            speakLocally(`${first} ${second}`, {onEnd, allowKoreanModel: true})
            await flush()

            expect(synthesizeKorean).not.toHaveBeenCalled()
            expect(loads).toHaveLength(2)
            expect(await playAll()).toBe(1)
            expect(onEnd).toHaveBeenCalledTimes(1)
        } finally {
            vi.unstubAllGlobals()
        }
    })

    it('does not start a second load while the Korean model is loading', async () => {
        getLocalKoTtsStatus.mockReturnValue('loading')

        speakLocally(korean('준비 중인 문장'), {})
        await flush()

        expect(loadLocalKoTts).not.toHaveBeenCalled()
        expect(synthesizeKorean).not.toHaveBeenCalled()
    })

    it('sentence is skipped when its synthesis rejects', async () => {
        const first = english('This one fails')
        const second = korean('이 문장은 읽습니다')
        synthesizeEnglish.mockRejectedValue(new Error('out of memory'))
        const onEnd = vi.fn()

        speakLocally(`${first} ${second}`, {onEnd})
        await flush()

        expect(await playAll()).toBe(1)
        expect(onEnd).toHaveBeenCalledTimes(1)
    })

    it('reports that nothing could be spoken when every synthesis fails', async () => {
        synthesizeEnglish.mockRejectedValue(new Error('out of memory'))
        const onEnd = vi.fn()
        const onUnavailable = vi.fn()

        speakLocally(english('Hello there'), {onEnd, onUnavailable})
        await flush()
        await flush()

        expect(onUnavailable).toHaveBeenCalledTimes(1)
        expect(onEnd).not.toHaveBeenCalled()
        expect(audioInstances.length).toBe(0)
    })

    it('reports that nothing could be spoken when this device cannot run the models', async () => {
        isLocalTtsSupported.mockReturnValue(false)
        const onUnavailable = vi.fn()

        speakLocally(english('Hello there'), {onUnavailable})
        await flush()

        expect(onUnavailable).toHaveBeenCalledTimes(1)
        expect(synthesizeEnglish).not.toHaveBeenCalled()
    })

    it('stop() during Korean synthesis prevents playback', async () => {
        let releaseKorean
        synthesizeKorean.mockImplementation(() => new Promise((resolve) => {
            releaseKorean = () => resolve({samples: new Float32Array([0.25]), sampleRate: KOREAN_RATE})
        }))
        const onEnd = vi.fn()
        const onUnavailable = vi.fn()

        const stop = speakLocally(korean('합성 중인 문장'), {onEnd, onUnavailable})
        await flush()
        stop()
        releaseKorean()
        await flush()

        expect(audioInstances.length).toBe(0)
        expect(onEnd).not.toHaveBeenCalled()
        expect(onUnavailable).not.toHaveBeenCalled()
    })

    it('stays silent after stop() during playback', async () => {
        const onEnd = vi.fn()

        const stop = speakLocally(`${english('First')} ${english('Second')}`, {onEnd})
        await flush()
        expect(audioInstances.length).toBe(1)
        stop()
        await flush()

        expect(audioInstances[0].paused).toBe(true)
        expect(audioInstances.length).toBe(1)
        expect(onEnd).not.toHaveBeenCalled()
    })

    it('second read of the same sentence reuses the cached audio', async () => {
        const englishSentence = english('Tell me about your routine')
        const koreanSentence = korean('좋은 답변이었습니다')
        const text = `${englishSentence} ${koreanSentence}`

        await prepareLocalSpeech(text)
        await flush()
        speakLocally(text, {})
        await flush()
        await playAll()
        speakLocally(text, {})
        await flush()

        expect(synthesizeEnglish).toHaveBeenCalledTimes(1)
        expect(synthesizeKorean).toHaveBeenCalledTimes(1)
        expect(audioInstances.length).toBe(3)
    })

    it('reports preparing and ready around synthesis', async () => {
        const onNotice = vi.fn()

        speakLocally(english('Describe your favorite park'), {onNotice})
        await flush()

        expect(onNotice.mock.calls.map(([notice]) => notice)).toEqual(['preparing', 'ready'])
    })

    it('speechSynthesis is never touched', async () => {
        window.speechSynthesis = {speak: vi.fn(), cancel: vi.fn(), getVoices: vi.fn(() => [])}

        speakLocally(`${english('Hello')} ${korean('안녕하세요')}`, {})
        await prepareLocalSpeech(english('Prepared'))
        await flush()
        await playAll()

        expect(window.speechSynthesis.speak).not.toHaveBeenCalled()
        expect(window.speechSynthesis.getVoices).not.toHaveBeenCalled()
    })
})

describe('neededLocalTtsModels', () => {
    beforeEach(() => {
        isLocalTtsSupported.mockReturnValue(true)
        isLocalKoTtsSupported.mockReturnValue(true)
    })

    it('needs both models for Korean feedback', () => {
        expect(neededLocalTtsModels('ko')).toEqual({english: true, korean: true})
    })

    it('needs only the English model for English feedback or no feedback', () => {
        expect(neededLocalTtsModels('en')).toEqual({english: true, korean: false})
        expect(neededLocalTtsModels(null)).toEqual({english: true, korean: false})
    })

    it('needs nothing this device cannot run', () => {
        isLocalTtsSupported.mockReturnValue(false)
        isLocalKoTtsSupported.mockReturnValue(false)

        expect(neededLocalTtsModels('ko')).toEqual({english: false, korean: false})
    })
})
