/** @vitest-environment jsdom */
import {act, renderHook, waitFor} from '@testing-library/react'
import {beforeEach, describe, expect, it, vi} from 'vitest'
import {useVoiceSession} from './useVoiceSession.js'
import {
    getVoiceEventsUrl,
    openVoiceSession,
    recoverVoiceSession,
    sendVoiceAnswer,
    sendVoiceAudioChunk,
    stopVoiceSession
} from '../../../shared/api/http.js'
import {loadLocalKoTts, loadLocalTts, neededLocalTtsModels, prepareLocalSpeech, speakLocally} from '../local/localSpeech.js'
import {loadLocalStt, transcribePcm16} from '../local/localStt.js'

vi.mock('../../../shared/api/http.js', () => ({
    openVoiceSession: vi.fn(),
    getVoiceEventsUrl: vi.fn(),
    recoverVoiceSession: vi.fn(),
    sendVoiceAnswer: vi.fn(),
    sendVoiceAudioChunk: vi.fn(),
    stopVoiceSession: vi.fn()
}))

vi.mock('../local/localSpeech.js', () => ({
    speakLocally: vi.fn(),
    prepareLocalSpeech: vi.fn(),
    neededLocalTtsModels: vi.fn(),
    loadLocalTts: vi.fn(),
    loadLocalKoTts: vi.fn()
}))

vi.mock('../local/localStt.js', () => ({
    loadLocalStt: vi.fn(),
    transcribePcm16: vi.fn()
}))

let activeEventSource = null
let spokenTexts = []
let speechCallbacks = []
let eventSourceInstances = []
let lastProcessor = null
let audioInstances = []
let audioPlayShouldFail = false

class MockEventSource {
    constructor(url) {
        this.url = url
        this.listeners = {}
        this.onerror = null
        this.closed = false
        activeEventSource = this
        eventSourceInstances.push(this)
    }

    addEventListener(type, listener) {
        if (!this.listeners[type]) {
            this.listeners[type] = []
        }
        this.listeners[type].push(listener)
    }

    close() {
        this.closed = true
    }

    emit(type, payload) {
        const event = {
            data: payload === undefined ? '' : JSON.stringify(payload)
        }
        const listeners = this.listeners[type] || []
        listeners.forEach((listener) => listener(event))
    }

    emitError(payload) {
        if (typeof this.onerror === 'function') {
            this.onerror({
                data: payload === undefined ? '' : JSON.stringify(payload)
            })
        }
    }
}

class MockScriptProcessorNode {
    constructor() {
        this.onaudioprocess = null
    }

    connect() {
    }

    disconnect() {
    }
}

class MockMediaStreamSourceNode {
    connect() {
    }

    disconnect() {
    }
}

class MockAudioContext {
    constructor({sampleRate} = {}) {
        this.sampleRate = sampleRate || 16000
        this.destination = {}
    }

    createMediaStreamSource() {
        return new MockMediaStreamSourceNode()
    }

    createScriptProcessor() {
        lastProcessor = new MockScriptProcessorNode()
        return lastProcessor
    }

    close() {
        return Promise.resolve()
    }
}

class MockAudio {
    constructor(src = '') {
        this.onended = null
        this.onerror = null
        this.src = src
        audioInstances.push(this)
    }

    play() {
        if (audioPlayShouldFail) {
            return Promise.reject(new Error('play failed'))
        }
        return Promise.resolve()
    }

    pause() {
    }
}

function emitAudioFrame(amplitude = 0) {
    if (!lastProcessor || typeof lastProcessor.onaudioprocess !== 'function') {
        throw new Error('audio processor is not initialized')
    }
    const samples = new Float32Array(16000)
    samples.fill(amplitude)
    lastProcessor.onaudioprocess({
        inputBuffer: {
            getChannelData: () => samples
        }
    })
}

function createRecoverySnapshot(overrides = {}) {
    return {
        sessionId: 's1',
        voiceSessionId: 'voice-1',
        active: true,
        stopped: false,
        stopReason: '',
        currentTurnId: 1,
        currentQuestion: null,
        turnProcessing: false,
        hasBufferedAudio: false,
        lastAcceptedChunkSequence: null,
        latestEventId: 0,
        replayFromEventId: 0,
        gapDetected: false,
        ...overrides
    }
}

function createAbortError() {
    const error = new Error('The operation was aborted.')
    error.name = 'AbortError'
    return error
}

async function completeQuestionPlayback({timersMocked = false} = {}) {
    const restoreRealTimers = !timersMocked
    if (restoreRealTimers) {
        vi.useFakeTimers()
    }
    try {
        const firstQuestionAudio = audioInstances[audioInstances.length - 1]
        if (typeof firstQuestionAudio?.onended === 'function') {
            await act(async () => {
                firstQuestionAudio.onended()
            })
        }
        await act(async () => {
            await vi.advanceTimersByTimeAsync(3000)
        })
        const repeatedQuestionAudio = audioInstances[audioInstances.length - 1]
        if (repeatedQuestionAudio !== firstQuestionAudio
            && typeof repeatedQuestionAudio?.onended === 'function') {
            await act(async () => {
                repeatedQuestionAudio.onended()
            })
        }
    } finally {
        if (restoreRealTimers) {
            vi.useRealTimers()
        }
    }
}

async function prepareConfirmedReplay({
                                          turnId = 1,
                                          questionId = 'q-1',
                                          questionText = 'question-1',
                                          sequence = 1
                                      } = {}) {
    await act(async () => {
        activeEventSource.emit('question.prompt', {
            turnId,
            question: {id: questionId, text: questionText, group: 'travel'},
            selection: {exhausted: false}
        })
        activeEventSource.emit('tts.audio', {
            role: 'question',
            audio: 'AQID',
            mimeType: 'audio/wav',
            sequence
        })
    })
    await completeQuestionPlayback()

    const uploadCallsBefore = sendVoiceAudioChunk.mock.calls.length
    act(() => {
        emitAudioFrame(0.1)
        emitAudioFrame(0)
        emitAudioFrame(0)
        emitAudioFrame(0)
    })
    await waitFor(() => {
        expect(sendVoiceAudioChunk.mock.calls.length).toBeGreaterThan(uploadCallsBefore)
    })
    await act(async () => {
        activeEventSource.emit('stt.final', {turnId, text: 'confirmed answer'})
    })
}

// The device voice reads a question twice, 3 seconds apart; the answer window opens after the second pass.
async function finishDeviceVoiceQuestion() {
    vi.useFakeTimers()
    try {
        await act(async () => {
            speechCallbacks[speechCallbacks.length - 1].onEnd()
        })
        await act(async () => {
            await vi.advanceTimersByTimeAsync(3000)
        })
        await act(async () => {
            speechCallbacks[speechCallbacks.length - 1].onEnd()
        })
    } finally {
        vi.useRealTimers()
    }
}

async function hearQuestionFromDeviceVoice({turnId = 1, questionId = 'q-1', sequence = turnId} = {}) {
    await act(async () => {
        activeEventSource.emit('question.prompt', {
            turnId,
            question: {id: questionId, text: `question-${turnId}`, group: 'travel'},
            selection: {exhausted: false}
        })
        activeEventSource.emit('tts.text', {role: 'question', text: `question-${turnId}`, sequence})
    })
    await finishDeviceVoiceQuestion()
}

function renderLocalSpeechSession({aiEnabled = true, ttsModel, feedbackLang = 'ko', onStatus = vi.fn()} = {}) {
    return renderHook(() => useVoiceSession({
        sessionId: 's1',
        feedbackModel: 'gpt-4o-mini',
        aiEnabled,
        autoReplay: false,
        voiceSttModel: 'gpt-4o-mini-transcribe',
        ttsModel,
        feedbackLang,
        voice: 'alloy',
        onStatus,
        onFeedback: vi.fn(),
        refreshWrongNotes: vi.fn(async () => {
        }),
        onQuestionPrompt: vi.fn()
    }))
}

function speakAnAnswer() {
    act(() => {
        emitAudioFrame(0.1)
        emitAudioFrame(0)
        emitAudioFrame(0)
        emitAudioFrame(0)
    })
}

// AI off: the question is read by the device voice, nothing is transcribed, and the server only
// acknowledges the end of the answer.
async function answerWithoutAi({turnId = 1, questionId = 'q-1'} = {}) {
    await hearQuestionFromDeviceVoice({turnId, questionId})
    const answersBefore = sendVoiceAnswer.mock.calls.length
    speakAnAnswer()
    await waitFor(() => {
        expect(sendVoiceAnswer.mock.calls.length).toBeGreaterThan(answersBefore)
    })
    await act(async () => {
        activeEventSource.emit('answer.captured', {turnId})
    })
}

describe('useVoiceSession', () => {
    beforeEach(() => {
        vi.clearAllMocks()

        activeEventSource = null
        eventSourceInstances = []
        lastProcessor = null
        audioInstances = []
        audioPlayShouldFail = false

        global.EventSource = MockEventSource
        global.Audio = MockAudio
        window.AudioContext = MockAudioContext
        window.webkitAudioContext = undefined

        window.URL.createObjectURL = vi.fn(() => 'blob://tts')
        window.URL.revokeObjectURL = vi.fn()

        const tracks = [{stop: vi.fn(), readyState: 'live', label: 'Mock Mic'}]
        const stream = {
            getTracks: () => tracks,
            getAudioTracks: () => tracks
        }

        Object.defineProperty(navigator, 'mediaDevices', {
            configurable: true,
            value: {
                getUserMedia: vi.fn(async () => stream),
                enumerateDevices: vi.fn(async () => [
                    {kind: 'audioinput', label: 'Mock Mic'},
                    {kind: 'audiooutput', label: 'Mock Speaker'}
                ]),
                addEventListener: vi.fn(),
                removeEventListener: vi.fn()
            }
        })

        openVoiceSession.mockResolvedValue({voiceSessionId: 'voice-1'})
        getVoiceEventsUrl.mockResolvedValue('http://localhost/events')
        recoverVoiceSession.mockResolvedValue(createRecoverySnapshot())
        sendVoiceAudioChunk.mockResolvedValue(undefined)
        sendVoiceAnswer.mockResolvedValue(undefined)
        stopVoiceSession.mockResolvedValue(undefined)

        spokenTexts = []
        speechCallbacks = []
        speakLocally.mockImplementation((text, callbacks) => {
            spokenTexts.push(text)
            speechCallbacks.push(callbacks)
            return vi.fn()
        })
        prepareLocalSpeech.mockResolvedValue(undefined)
        neededLocalTtsModels.mockImplementation((feedbackLanguage) => ({english: true, korean: feedbackLanguage === 'ko'}))
        loadLocalTts.mockResolvedValue(undefined)
        loadLocalKoTts.mockResolvedValue(undefined)
        loadLocalStt.mockResolvedValue(undefined)
        transcribePcm16.mockResolvedValue('local transcript')
    })

    it('sends selected models when opening voice session', async () => {
        const {result, unmount} = renderHook(() => useVoiceSession({
            sessionId: 's1',
            feedbackModel: 'gpt-5-mini',
            voiceSttModel: 'gpt-4o-transcribe',
            ttsModel: 'tts-1-hd',
            feedbackLang: 'ko',
            voice: 'alloy',
            onStatus: vi.fn(),
            onFeedback: vi.fn(),
            refreshWrongNotes: vi.fn(async () => {
            }),
            onQuestionPrompt: vi.fn()
        }))

        await act(async () => {
            await result.current.startSession()
        })

        expect(openVoiceSession).toHaveBeenCalledWith(expect.objectContaining({
            sessionId: 's1',
            feedbackModel: 'gpt-5-mini',
            feedbackLanguage: 'ko',
            sttModel: 'gpt-4o-transcribe',
            ttsModel: 'tts-1-hd',
            ttsVoice: 'alloy'
        }))
        unmount()
    })

    it('blocks STT transmission while question tts is still playing', async () => {
        const {result, unmount} = renderHook(() => useVoiceSession({
            sessionId: 's1',
            feedbackModel: 'gpt-4o-mini',
            voiceSttModel: 'gpt-4o-mini-transcribe',
            feedbackLang: 'ko',
            voice: 'alloy',
            onStatus: vi.fn(),
            onFeedback: vi.fn(),
            refreshWrongNotes: vi.fn(async () => {
            }),
            onQuestionPrompt: vi.fn()
        }))

        await act(async () => {
            await result.current.startSession()
        })

        await act(async () => {
            activeEventSource.emit('tts.audio', {
                role: 'question',
                audio: 'AQID',
                mimeType: 'audio/wav',
                sequence: 1
            })
        })

        expect(result.current.speechState).toBe('PLAYING_QUESTION_TTS')

        act(() => {
            emitAudioFrame(0.1)
            emitAudioFrame(0)
            emitAudioFrame(0)
            emitAudioFrame(0)
        })

        await act(async () => {
            await Promise.resolve()
        })

        expect(sendVoiceAudioChunk).not.toHaveBeenCalled()
        unmount()
    })

    it('sends STT audio only after question tts playback completes', async () => {
        const {result, unmount} = renderHook(() => useVoiceSession({
            sessionId: 's1',
            feedbackModel: 'gpt-4o-mini',
            voiceSttModel: 'gpt-4o-mini-transcribe',
            feedbackLang: 'ko',
            voice: 'alloy',
            onStatus: vi.fn(),
            onFeedback: vi.fn(),
            refreshWrongNotes: vi.fn(async () => {
            }),
            onQuestionPrompt: vi.fn()
        }))

        await act(async () => {
            await result.current.startSession()
        })

        await act(async () => {
            activeEventSource.emit('tts.audio', {
                role: 'question',
                audio: 'AQID',
                mimeType: 'audio/wav',
                sequence: 1
            })
        })

        await completeQuestionPlayback()

        expect(result.current.speechState).toBe('READY_FOR_ANSWER')

        act(() => {
            emitAudioFrame(0.1)
            emitAudioFrame(0)
            emitAudioFrame(0)
            emitAudioFrame(0)
        })

        await waitFor(() => {
            expect(sendVoiceAudioChunk).toHaveBeenCalledTimes(1)
        })
        expect(sendVoiceAudioChunk).toHaveBeenCalledWith(
            'voice-1',
            expect.objectContaining({
                sampleRate: 16000,
                sequence: 1
            }),
            expect.objectContaining({
                signal: expect.any(Object)
            })
        )
        unmount()
    })

    it('replays question tts 3 seconds later before opening answer capture', async () => {
        vi.useFakeTimers()
        try {
            const {result, unmount} = renderHook(() => useVoiceSession({
                sessionId: 's1',
                feedbackModel: 'gpt-4o-mini',
                voiceSttModel: 'gpt-4o-mini-transcribe',
                feedbackLang: 'ko',
                voice: 'alloy',
                onStatus: vi.fn(),
                onFeedback: vi.fn(),
                refreshWrongNotes: vi.fn(async () => {
                }),
                onQuestionPrompt: vi.fn()
            }))

            await act(async () => {
                await result.current.startSession()
            })

            await act(async () => {
                activeEventSource.emit('tts.audio', {
                    role: 'question',
                    audio: 'AQID',
                    mimeType: 'audio/wav',
                    sequence: 1
                })
            })

            expect(audioInstances.length).toBe(1)
            const firstQuestionAudio = audioInstances[audioInstances.length - 1]
            await act(async () => {
                firstQuestionAudio.onended()
            })
            expect(result.current.speechState).toBe('PLAYING_QUESTION_TTS')

            await act(async () => {
                await vi.advanceTimersByTimeAsync(2999)
            })
            expect(audioInstances.length).toBe(1)
            expect(result.current.speechState).toBe('PLAYING_QUESTION_TTS')

            await act(async () => {
                await vi.advanceTimersByTimeAsync(1)
            })
            expect(audioInstances.length).toBe(2)
            const repeatedQuestionAudio = audioInstances[audioInstances.length - 1]
            expect(repeatedQuestionAudio).not.toBe(firstQuestionAudio)
            expect(result.current.speechState).toBe('PLAYING_QUESTION_TTS')

            await act(async () => {
                repeatedQuestionAudio.onended()
            })
            expect(result.current.speechState).toBe('READY_FOR_ANSWER')
            unmount()
        } finally {
            vi.useRealTimers()
        }
    })

    it('stops session immediately when tts error arrives', async () => {
        const {result, unmount} = renderHook(() => useVoiceSession({
            sessionId: 's1',
            feedbackModel: 'gpt-4o-mini',
            voiceSttModel: 'gpt-4o-mini-transcribe',
            feedbackLang: 'ko',
            voice: 'alloy',
            onStatus: vi.fn(),
            onFeedback: vi.fn(),
            refreshWrongNotes: vi.fn(async () => {
            }),
            onQuestionPrompt: vi.fn()
        }))

        await act(async () => {
            await result.current.startSession()
        })

        await act(async () => {
            activeEventSource.emit('tts.error', {message: 'synthesis failed'})
        })

        await waitFor(() => {
            expect(stopVoiceSession).toHaveBeenCalledWith(
                'voice-1',
                {forced: true, reason: 'tts_failed'}
            )
        })
        expect(result.current.speechState).toBe('IDLE')
        unmount()
    })

    it('retries event stream connection before stopping the session', async () => {
        vi.useFakeTimers()
        const onStatus = vi.fn()
        const {result, unmount} = renderHook(() => useVoiceSession({
            sessionId: 's1',
            feedbackModel: 'gpt-4o-mini',
            voiceSttModel: 'gpt-4o-mini-transcribe',
            feedbackLang: 'ko',
            voice: 'alloy',
            onStatus,
            onFeedback: vi.fn(),
            refreshWrongNotes: vi.fn(async () => {
            }),
            onQuestionPrompt: vi.fn()
        }))

        await act(async () => {
            await result.current.startSession()
        })
        expect(eventSourceInstances).toHaveLength(1)

        act(() => {
            activeEventSource.emitError()
        })

        expect(stopVoiceSession).not.toHaveBeenCalled()
        expect(result.current.sessionActive).toBe(true)

        await act(async () => {
            await vi.advanceTimersByTimeAsync(500)
        })

        expect(eventSourceInstances).toHaveLength(2)
        expect(recoverVoiceSession).toHaveBeenCalledWith('voice-1', 0)
        expect(stopVoiceSession).not.toHaveBeenCalled()

        await act(async () => {
            activeEventSource.emit('session.ready', {sessionId: 's1', voiceSessionId: 'voice-1'})
        })
        expect(result.current.voiceConnected).toBe(true)
        expect(onStatus).toHaveBeenCalledWith('음성 이벤트 연결이 복구되었습니다.')
        unmount()
        vi.useRealTimers()
    })

    it('stops session when reconnect attempts are exhausted', async () => {
        vi.useFakeTimers()
        const {result, unmount} = renderHook(() => useVoiceSession({
            sessionId: 's1',
            feedbackModel: 'gpt-4o-mini',
            voiceSttModel: 'gpt-4o-mini-transcribe',
            feedbackLang: 'ko',
            voice: 'alloy',
            onStatus: vi.fn(),
            onFeedback: vi.fn(),
            refreshWrongNotes: vi.fn(async () => {
            }),
            onQuestionPrompt: vi.fn()
        }))

        await act(async () => {
            await result.current.startSession()
        })

        getVoiceEventsUrl.mockRejectedValue(new Error('sse unavailable'))

        act(() => {
            activeEventSource.emitError()
        })

        const reconnectDelays = [500, 1000, 2000, 4000]
        for (const delay of reconnectDelays) {
            await act(async () => {
                await vi.advanceTimersByTimeAsync(delay)
            })
        }

        await act(async () => {
            await Promise.resolve()
        })

        expect(stopVoiceSession).not.toHaveBeenCalled()
        expect(result.current.sessionActive).toBe(false)
        expect(result.current.speechState).toBe('IDLE')
        unmount()
        vi.useRealTimers()
    })

    it('does not reconnect when disconnect happens after local stop', async () => {
        vi.useFakeTimers()
        const {result, unmount} = renderHook(() => useVoiceSession({
            sessionId: 's1',
            feedbackModel: 'gpt-4o-mini',
            voiceSttModel: 'gpt-4o-mini-transcribe',
            feedbackLang: 'ko',
            voice: 'alloy',
            onStatus: vi.fn(),
            onFeedback: vi.fn(),
            refreshWrongNotes: vi.fn(async () => {
            }),
            onQuestionPrompt: vi.fn()
        }))

        await act(async () => {
            await result.current.startSession()
        })
        const sourceCountBeforeStop = eventSourceInstances.length

        await act(async () => {
            await result.current.stopSession()
        })
        expect(stopVoiceSession).toHaveBeenCalledTimes(1)

        act(() => {
            activeEventSource.emitError()
        })
        await act(async () => {
            await vi.advanceTimersByTimeAsync(5000)
        })

        expect(eventSourceInstances).toHaveLength(sourceCountBeforeStop)
        unmount()
        vi.useRealTimers()
    })

    it('syncs from recovery snapshot before reconnecting event stream', async () => {
        vi.useFakeTimers()
        const onStatus = vi.fn()
        const onQuestionPrompt = vi.fn()
        recoverVoiceSession.mockResolvedValueOnce({
            sessionId: 's1',
            voiceSessionId: 'voice-1',
            active: true,
            stopped: false,
            stopReason: '',
            currentTurnId: 5,
            currentQuestion: {
                id: 'q-2',
                text: 'recovered question',
                group: 'travel',
                groupId: 'g-2',
                questionType: 'OPEN'
            },
            turnProcessing: false,
            hasBufferedAudio: false,
            lastAcceptedChunkSequence: 3,
            latestEventId: 8,
            replayFromEventId: 4,
            gapDetected: true
        })

        const {result, unmount} = renderHook(() => useVoiceSession({
            sessionId: 's1',
            feedbackModel: 'gpt-4o-mini',
            voiceSttModel: 'gpt-4o-mini-transcribe',
            feedbackLang: 'ko',
            voice: 'alloy',
            onStatus,
            onFeedback: vi.fn(),
            refreshWrongNotes: vi.fn(async () => {
            }),
            onQuestionPrompt
        }))

        await act(async () => {
            await result.current.startSession()
        })

        act(() => {
            activeEventSource.emitError()
        })

        await act(async () => {
            await vi.advanceTimersByTimeAsync(500)
        })

        expect(recoverVoiceSession).toHaveBeenCalledWith('voice-1', 0)
        expect(getVoiceEventsUrl).toHaveBeenLastCalledWith('voice-1', 4)
        expect(onQuestionPrompt).toHaveBeenCalledWith({
            turnId: 5,
            questionId: 'q-2',
            text: 'recovered question',
            group: 'travel',
            exhausted: false,
            selectionReason: ''
        })
        expect(onStatus).toHaveBeenCalledWith('이벤트 일부가 누락되어 서버 상태 기준으로 동기화한 뒤 연결을 복구했습니다.')
        expect(result.current.speechState).toBe('READY_FOR_ANSWER')
        unmount()
        vi.useRealTimers()
    })

    it('re-uploads in-flight turn after reconnect when question context matches', async () => {
        vi.useFakeTimers()
        try {
            const onStatus = vi.fn()
            recoverVoiceSession.mockResolvedValueOnce(createRecoverySnapshot({
                currentTurnId: 5,
                currentQuestion: {
                    id: 'q-1',
                    text: 'recovered question',
                    group: 'travel',
                    groupId: 'g-1',
                    questionType: 'OPEN'
                }
            }))

            const {result, unmount} = renderHook(() => useVoiceSession({
                sessionId: 's1',
                feedbackModel: 'gpt-4o-mini',
                voiceSttModel: 'gpt-4o-mini-transcribe',
                feedbackLang: 'ko',
                voice: 'alloy',
                onStatus,
                onFeedback: vi.fn(),
                refreshWrongNotes: vi.fn(async () => {
                }),
                onQuestionPrompt: vi.fn()
            }))

            await act(async () => {
                await result.current.startSession()
            })
            await act(async () => {
                activeEventSource.emit('question.prompt', {
                    question: {id: 'q-1', text: 'question-1', group: 'travel'},
                    selection: {exhausted: false}
                })
                activeEventSource.emit('tts.audio', {
                    role: 'question',
                    audio: 'AQID',
                    mimeType: 'audio/wav',
                    sequence: 1
                })
            })
            await completeQuestionPlayback({timersMocked: true})

            sendVoiceAudioChunk.mockRejectedValueOnce(new Error('temporary network'))
            act(() => {
                emitAudioFrame(0.1)
                emitAudioFrame(0)
                emitAudioFrame(0)
                emitAudioFrame(0)
            })
            await act(async () => {
                await Promise.resolve()
            })
            expect(sendVoiceAudioChunk).toHaveBeenCalledTimes(1)
            const firstSequence = sendVoiceAudioChunk.mock.calls[0][1].sequence

            act(() => {
                activeEventSource.emitError()
            })
            await act(async () => {
                await vi.advanceTimersByTimeAsync(500)
            })
            await act(async () => {
                await Promise.resolve()
            })
            if (sendVoiceAudioChunk.mock.calls.length < 2) {
                await act(async () => {
                    await vi.advanceTimersByTimeAsync(500)
                })
                await act(async () => {
                    await Promise.resolve()
                })
            }
            expect(sendVoiceAudioChunk).toHaveBeenCalledTimes(2)

            const secondSequence = sendVoiceAudioChunk.mock.calls[1][1].sequence
            expect(secondSequence).toBe(firstSequence)
            expect(onStatus).toHaveBeenCalledWith('음성 이벤트 연결이 복구되었습니다.')
            unmount()
        } finally {
            vi.useRealTimers()
        }
    })

    it('aborts stalled in-flight upload on disconnect and re-uploads after recovery', async () => {
        vi.useFakeTimers()
        try {
            const onStatus = vi.fn()
            recoverVoiceSession.mockResolvedValueOnce(createRecoverySnapshot({
                currentTurnId: 5,
                currentQuestion: {
                    id: 'q-1',
                    text: 'recovered question',
                    group: 'travel',
                    groupId: 'g-1',
                    questionType: 'OPEN'
                }
            }))

            let uploadCallCount = 0
            sendVoiceAudioChunk.mockImplementation((_voiceSessionId, _payload, requestOptions = {}) => {
                uploadCallCount += 1
                if (uploadCallCount > 1) {
                    return Promise.resolve(undefined)
                }
                return new Promise((_resolve, reject) => {
                    const signal = requestOptions?.signal
                    if (!signal) {
                        return
                    }
                    if (signal.aborted) {
                        reject(createAbortError())
                        return
                    }
                    signal.addEventListener('abort', () => {
                        reject(createAbortError())
                    }, {once: true})
                })
            })

            const {result, unmount} = renderHook(() => useVoiceSession({
                sessionId: 's1',
                feedbackModel: 'gpt-4o-mini',
                voiceSttModel: 'gpt-4o-mini-transcribe',
                feedbackLang: 'ko',
                voice: 'alloy',
                onStatus,
                onFeedback: vi.fn(),
                refreshWrongNotes: vi.fn(async () => {
                }),
                onQuestionPrompt: vi.fn()
            }))

            await act(async () => {
                await result.current.startSession()
            })
            await act(async () => {
                activeEventSource.emit('question.prompt', {
                    question: {id: 'q-1', text: 'question-1', group: 'travel'},
                    selection: {exhausted: false}
                })
                activeEventSource.emit('tts.audio', {
                    role: 'question',
                    audio: 'AQID',
                    mimeType: 'audio/wav',
                    sequence: 1
                })
            })
            await completeQuestionPlayback({timersMocked: true})

            act(() => {
                emitAudioFrame(0.1)
                emitAudioFrame(0)
                emitAudioFrame(0)
                emitAudioFrame(0)
            })
            await act(async () => {
                await Promise.resolve()
            })
            expect(sendVoiceAudioChunk).toHaveBeenCalledTimes(1)

            const firstSequence = sendVoiceAudioChunk.mock.calls[0][1].sequence
            const firstSignal = sendVoiceAudioChunk.mock.calls[0][2]?.signal
            expect(firstSignal?.aborted).toBe(false)

            act(() => {
                activeEventSource.emitError()
            })
            await act(async () => {
                await Promise.resolve()
            })

            expect(firstSignal?.aborted).toBe(true)
            expect(onStatus).toHaveBeenCalledWith('오디오 전송이 지연되어 재시도합니다.')

            await act(async () => {
                await vi.advanceTimersByTimeAsync(500)
            })
            await act(async () => {
                await Promise.resolve()
            })
            if (sendVoiceAudioChunk.mock.calls.length < 2) {
                await act(async () => {
                    await vi.advanceTimersByTimeAsync(500)
                })
                await act(async () => {
                    await Promise.resolve()
                })
            }

            expect(sendVoiceAudioChunk).toHaveBeenCalledTimes(2)
            const secondSequence = sendVoiceAudioChunk.mock.calls[1][1].sequence
            expect(secondSequence).toBe(firstSequence)
            expect(onStatus).toHaveBeenCalledWith('음성 이벤트 연결이 복구되었습니다.')
            unmount()
        } finally {
            vi.useRealTimers()
        }
    })

    it('discards buffered audio after reconnect when question context mismatches', async () => {
        vi.useFakeTimers()
        try {
            const onStatus = vi.fn()
            recoverVoiceSession.mockResolvedValueOnce(createRecoverySnapshot({
                currentTurnId: 5,
                currentQuestion: {
                    id: 'q-2',
                    text: 'different question',
                    group: 'travel',
                    groupId: 'g-2',
                    questionType: 'OPEN'
                }
            }))

            const {result, unmount} = renderHook(() => useVoiceSession({
                sessionId: 's1',
                feedbackModel: 'gpt-4o-mini',
                voiceSttModel: 'gpt-4o-mini-transcribe',
                feedbackLang: 'ko',
                voice: 'alloy',
                onStatus,
                onFeedback: vi.fn(),
                refreshWrongNotes: vi.fn(async () => {
                }),
                onQuestionPrompt: vi.fn()
            }))

            await act(async () => {
                await result.current.startSession()
            })
            await act(async () => {
                activeEventSource.emit('question.prompt', {
                    question: {id: 'q-1', text: 'question-1', group: 'travel'},
                    selection: {exhausted: false}
                })
                activeEventSource.emit('tts.audio', {
                    role: 'question',
                    audio: 'AQID',
                    mimeType: 'audio/wav',
                    sequence: 1
                })
            })
            await completeQuestionPlayback({timersMocked: true})

            sendVoiceAudioChunk.mockRejectedValueOnce(new Error('temporary network'))
            act(() => {
                emitAudioFrame(0.1)
                emitAudioFrame(0)
                emitAudioFrame(0)
                emitAudioFrame(0)
            })
            await act(async () => {
                await Promise.resolve()
            })
            expect(sendVoiceAudioChunk).toHaveBeenCalledTimes(1)

            act(() => {
                activeEventSource.emitError()
            })
            await act(async () => {
                await vi.advanceTimersByTimeAsync(500)
            })
            await act(async () => {
                await Promise.resolve()
            })

            expect(sendVoiceAudioChunk).toHaveBeenCalledTimes(1)
            expect(onStatus).toHaveBeenCalledWith(
                '음성 이벤트 연결은 복구되었지만 질문 컨텍스트가 달라 미전송 음성을 폐기했습니다. 현재 질문에 다시 답변해 주세요.'
            )
            unmount()
        } finally {
            vi.useRealTimers()
        }
    })

    it('skips re-upload when server already accepted in-flight sequence', async () => {
        vi.useFakeTimers()
        try {
            const onStatus = vi.fn()

            const {result, unmount} = renderHook(() => useVoiceSession({
                sessionId: 's1',
                feedbackModel: 'gpt-4o-mini',
                voiceSttModel: 'gpt-4o-mini-transcribe',
                feedbackLang: 'ko',
                voice: 'alloy',
                onStatus,
                onFeedback: vi.fn(),
                refreshWrongNotes: vi.fn(async () => {
                }),
                onQuestionPrompt: vi.fn()
            }))

            await act(async () => {
                await result.current.startSession()
            })
            await act(async () => {
                activeEventSource.emit('question.prompt', {
                    question: {id: 'q-1', text: 'question-1', group: 'travel'},
                    selection: {exhausted: false}
                })
                activeEventSource.emit('tts.audio', {
                    role: 'question',
                    audio: 'AQID',
                    mimeType: 'audio/wav',
                    sequence: 1
                })
            })
            await completeQuestionPlayback({timersMocked: true})

            sendVoiceAudioChunk.mockRejectedValueOnce(new Error('temporary network'))
            act(() => {
                emitAudioFrame(0.1)
                emitAudioFrame(0)
                emitAudioFrame(0)
                emitAudioFrame(0)
            })
            await act(async () => {
                await Promise.resolve()
            })
            expect(sendVoiceAudioChunk).toHaveBeenCalledTimes(1)
            const firstSequence = sendVoiceAudioChunk.mock.calls[0][1].sequence

            recoverVoiceSession.mockResolvedValueOnce(createRecoverySnapshot({
                currentTurnId: 5,
                currentQuestion: {
                    id: 'q-1',
                    text: 'same question',
                    group: 'travel',
                    groupId: 'g-1',
                    questionType: 'OPEN'
                },
                lastAcceptedChunkSequence: firstSequence
            }))

            act(() => {
                activeEventSource.emitError()
            })
            await act(async () => {
                await vi.advanceTimersByTimeAsync(500)
            })
            await act(async () => {
                await Promise.resolve()
            })

            expect(sendVoiceAudioChunk).toHaveBeenCalledTimes(1)
            expect(onStatus).toHaveBeenCalledWith('음성 이벤트 연결이 복구되었습니다.')
            unmount()
        } finally {
            vi.useRealTimers()
        }
    })

    it('ignores duplicate events by eventId', async () => {
        const onFeedback = vi.fn()
        const {result, unmount} = renderHook(() => useVoiceSession({
            sessionId: 's1',
            feedbackModel: 'gpt-4o-mini',
            voiceSttModel: 'gpt-4o-mini-transcribe',
            feedbackLang: 'ko',
            voice: 'alloy',
            onStatus: vi.fn(),
            onFeedback,
            refreshWrongNotes: vi.fn(async () => {
            }),
            onQuestionPrompt: vi.fn()
        }))

        await act(async () => {
            await result.current.startSession()
        })

        await act(async () => {
            activeEventSource.emit('feedback.final', {eventId: 11, summary: 'first'})
            activeEventSource.emit('feedback.final', {eventId: 11, summary: 'second'})
            activeEventSource.emit('feedback.final', {eventId: 12, summary: 'third'})
        })

        expect(onFeedback).toHaveBeenCalledTimes(2)
        expect(onFeedback.mock.calls[0][0].summary).toBe('first')
        expect(onFeedback.mock.calls[1][0].summary).toBe('third')
        unmount()
    })

    it('returns to READY_FOR_ANSWER when stt.skipped is received', async () => {
        const onStatus = vi.fn()
        const {result, unmount} = renderHook(() => useVoiceSession({
            sessionId: 's1',
            feedbackModel: 'gpt-4o-mini',
            voiceSttModel: 'gpt-4o-mini-transcribe',
            feedbackLang: 'ko',
            voice: 'alloy',
            onStatus,
            onFeedback: vi.fn(),
            refreshWrongNotes: vi.fn(async () => {
            }),
            onQuestionPrompt: vi.fn()
        }))

        await act(async () => {
            await result.current.startSession()
        })
        await act(async () => {
            activeEventSource.emit('tts.audio', {
                eventId: 1,
                role: 'question',
                audio: 'AQID',
                mimeType: 'audio/wav',
                sequence: 1
            })
        })
        await completeQuestionPlayback()
        expect(result.current.speechState).toBe('READY_FOR_ANSWER')

        act(() => {
            emitAudioFrame(0.1)
        })
        expect(result.current.speechState).toBe('CAPTURING_ANSWER')

        await act(async () => {
            activeEventSource.emit('stt.skipped', {eventId: 2, reason: 'empty_transcript'})
        })

        expect(result.current.speechState).toBe('READY_FOR_ANSWER')
        expect(onStatus).toHaveBeenCalledWith('음성이 인식되지 않아 다시 답변해 주세요.')
        unmount()
    })

    it('drops buffered audio immediately when question prompt arrives', async () => {
        const {result, unmount} = renderHook(() => useVoiceSession({
            sessionId: 's1',
            feedbackModel: 'gpt-4o-mini',
            voiceSttModel: 'gpt-4o-mini-transcribe',
            feedbackLang: 'ko',
            voice: 'alloy',
            onStatus: vi.fn(),
            onFeedback: vi.fn(),
            refreshWrongNotes: vi.fn(async () => {
            }),
            onQuestionPrompt: vi.fn()
        }))

        await act(async () => {
            await result.current.startSession()
        })

        await act(async () => {
            activeEventSource.emit('tts.audio', {
                role: 'question',
                audio: 'AQID',
                mimeType: 'audio/wav',
                sequence: 1
            })
        })
        await completeQuestionPlayback()
        expect(result.current.speechState).toBe('READY_FOR_ANSWER')

        act(() => {
            emitAudioFrame(0.1)
        })
        expect(result.current.speechState).toBe('CAPTURING_ANSWER')

        await act(async () => {
            activeEventSource.emit('question.prompt', {
                question: {id: 'q-2', text: 'next question'},
                selection: {exhausted: false}
            })
        })
        expect(result.current.speechState).toBe('WAITING_TTS')

        act(() => {
            emitAudioFrame(0)
            emitAudioFrame(0)
            emitAudioFrame(0)
        })
        await act(async () => {
            await Promise.resolve()
        })
        expect(sendVoiceAudioChunk).not.toHaveBeenCalled()
        unmount()
    })

    it('drops buffered audio when feedback tts starts', async () => {
        const {result, unmount} = renderHook(() => useVoiceSession({
            sessionId: 's1',
            feedbackModel: 'gpt-4o-mini',
            voiceSttModel: 'gpt-4o-mini-transcribe',
            feedbackLang: 'ko',
            voice: 'alloy',
            onStatus: vi.fn(),
            onFeedback: vi.fn(),
            refreshWrongNotes: vi.fn(async () => {
            }),
            onQuestionPrompt: vi.fn()
        }))

        await act(async () => {
            await result.current.startSession()
        })

        await act(async () => {
            activeEventSource.emit('tts.audio', {
                role: 'question',
                audio: 'AQID',
                mimeType: 'audio/wav',
                sequence: 1
            })
        })
        await completeQuestionPlayback()
        expect(result.current.speechState).toBe('READY_FOR_ANSWER')

        act(() => {
            emitAudioFrame(0.1)
        })
        expect(result.current.speechState).toBe('CAPTURING_ANSWER')

        await act(async () => {
            activeEventSource.emit('tts.audio', {
                role: 'feedback',
                audio: 'AQID',
                mimeType: 'audio/wav',
                sequence: 2
            })
        })
        expect(result.current.speechState).toBe('PLAYING_FEEDBACK_TTS')

        act(() => {
            emitAudioFrame(0)
            emitAudioFrame(0)
            emitAudioFrame(0)
        })
        await act(async () => {
            await Promise.resolve()
        })
        expect(sendVoiceAudioChunk).not.toHaveBeenCalled()
        unmount()
    })

    it('does not carry previous turn residue into the next question answer', async () => {
        const {result, unmount} = renderHook(() => useVoiceSession({
            sessionId: 's1',
            feedbackModel: 'gpt-4o-mini',
            voiceSttModel: 'gpt-4o-mini-transcribe',
            feedbackLang: 'ko',
            voice: 'alloy',
            onStatus: vi.fn(),
            onFeedback: vi.fn(),
            refreshWrongNotes: vi.fn(async () => {
            }),
            onQuestionPrompt: vi.fn()
        }))

        await act(async () => {
            await result.current.startSession()
        })

        // Question 1 ready
        await act(async () => {
            activeEventSource.emit('tts.audio', {
                role: 'question',
                audio: 'AQID',
                mimeType: 'audio/wav',
                sequence: 1
            })
        })
        await completeQuestionPlayback()
        expect(result.current.speechState).toBe('READY_FOR_ANSWER')

        // Start speaking for question 1, but switch to next question before silence flush.
        act(() => {
            emitAudioFrame(0.1)
        })
        expect(result.current.speechState).toBe('CAPTURING_ANSWER')

        await act(async () => {
            activeEventSource.emit('question.prompt', {
                question: {id: 'q-2', text: 'next question'},
                selection: {exhausted: false}
            })
            activeEventSource.emit('tts.audio', {
                role: 'question',
                audio: 'AQID',
                mimeType: 'audio/wav',
                sequence: 2
            })
        })
        await completeQuestionPlayback()
        expect(result.current.speechState).toBe('READY_FOR_ANSWER')

        // Speak for question 2 only.
        act(() => {
            emitAudioFrame(0.1)
            emitAudioFrame(0)
            emitAudioFrame(0)
            emitAudioFrame(0)
        })

        await waitFor(() => {
            expect(sendVoiceAudioChunk).toHaveBeenCalledTimes(1)
        })
        unmount()
    })

    it('retries chunk upload with the same sequence until it succeeds', async () => {
        vi.useFakeTimers()
        try {
            const {result, unmount} = renderHook(() => useVoiceSession({
                sessionId: 's1',
                feedbackModel: 'gpt-4o-mini',
                voiceSttModel: 'gpt-4o-mini-transcribe',
                feedbackLang: 'ko',
                voice: 'alloy',
                onStatus: vi.fn(),
                onFeedback: vi.fn(),
                refreshWrongNotes: vi.fn(async () => {
                }),
                onQuestionPrompt: vi.fn()
            }))

            await act(async () => {
                await result.current.startSession()
            })
            await act(async () => {
                activeEventSource.emit('tts.audio', {
                    role: 'question',
                    audio: 'AQID',
                    mimeType: 'audio/wav',
                    sequence: 1
                })
            })
            await completeQuestionPlayback({timersMocked: true})

            sendVoiceAudioChunk
                .mockRejectedValueOnce(new Error('temporary'))
                .mockResolvedValueOnce(undefined)

            act(() => {
                emitAudioFrame(0.1)
                emitAudioFrame(0)
                emitAudioFrame(0)
                emitAudioFrame(0)
            })

            await act(async () => {
                await Promise.resolve()
            })
            expect(sendVoiceAudioChunk).toHaveBeenCalledTimes(1)

            const firstSequence = sendVoiceAudioChunk.mock.calls[0][1].sequence

            await act(async () => {
                await vi.advanceTimersByTimeAsync(500)
            })
            await act(async () => {
                await Promise.resolve()
            })

            expect(sendVoiceAudioChunk).toHaveBeenCalledTimes(2)

            const secondSequence = sendVoiceAudioChunk.mock.calls[1][1].sequence
            expect(secondSequence).toBe(firstSequence)
            expect(result.current.sessionActive).toBe(true)
            unmount()
        } finally {
            vi.useRealTimers()
        }
    })

    it('aborts stalled chunk upload after 3 seconds and retries', async () => {
        vi.useFakeTimers()
        try {
            const onStatus = vi.fn()
            let uploadCallCount = 0
            sendVoiceAudioChunk.mockImplementation((_voiceSessionId, _payload, requestOptions = {}) => {
                uploadCallCount += 1
                if (uploadCallCount > 1) {
                    return Promise.resolve(undefined)
                }
                return new Promise((_resolve, reject) => {
                    const signal = requestOptions?.signal
                    if (!signal) {
                        return
                    }
                    if (signal.aborted) {
                        reject(createAbortError())
                        return
                    }
                    signal.addEventListener('abort', () => {
                        reject(createAbortError())
                    }, {once: true})
                })
            })

            const {result, unmount} = renderHook(() => useVoiceSession({
                sessionId: 's1',
                feedbackModel: 'gpt-4o-mini',
                voiceSttModel: 'gpt-4o-mini-transcribe',
                feedbackLang: 'ko',
                voice: 'alloy',
                onStatus,
                onFeedback: vi.fn(),
                refreshWrongNotes: vi.fn(async () => {
                }),
                onQuestionPrompt: vi.fn()
            }))

            await act(async () => {
                await result.current.startSession()
            })
            await act(async () => {
                activeEventSource.emit('tts.audio', {
                    role: 'question',
                    audio: 'AQID',
                    mimeType: 'audio/wav',
                    sequence: 1
                })
            })
            await completeQuestionPlayback({timersMocked: true})

            act(() => {
                emitAudioFrame(0.1)
                emitAudioFrame(0)
                emitAudioFrame(0)
                emitAudioFrame(0)
            })
            await act(async () => {
                await Promise.resolve()
            })
            expect(sendVoiceAudioChunk).toHaveBeenCalledTimes(1)
            const firstSequence = sendVoiceAudioChunk.mock.calls[0][1].sequence
            const firstSignal = sendVoiceAudioChunk.mock.calls[0][2]?.signal
            expect(firstSignal?.aborted).toBe(false)

            await act(async () => {
                await vi.advanceTimersByTimeAsync(3000)
            })
            await act(async () => {
                await Promise.resolve()
            })
            expect(firstSignal?.aborted).toBe(true)
            expect(onStatus).toHaveBeenCalledWith('오디오 전송이 지연되어 재시도합니다.')

            await act(async () => {
                await vi.advanceTimersByTimeAsync(500)
            })
            await act(async () => {
                await Promise.resolve()
            })

            expect(sendVoiceAudioChunk).toHaveBeenCalledTimes(2)
            const secondSequence = sendVoiceAudioChunk.mock.calls[1][1].sequence
            expect(secondSequence).toBe(firstSequence)
            expect(result.current.sessionActive).toBe(true)
            unmount()
        } finally {
            vi.useRealTimers()
        }
    })

    it('stops session after chunk upload retries are exhausted', async () => {
        vi.useFakeTimers()
        try {
            const onStatus = vi.fn()
            const {result, unmount} = renderHook(() => useVoiceSession({
                sessionId: 's1',
                feedbackModel: 'gpt-4o-mini',
                voiceSttModel: 'gpt-4o-mini-transcribe',
                feedbackLang: 'ko',
                voice: 'alloy',
                onStatus,
                onFeedback: vi.fn(),
                refreshWrongNotes: vi.fn(async () => {
                }),
                onQuestionPrompt: vi.fn()
            }))

            await act(async () => {
                await result.current.startSession()
            })
            await act(async () => {
                activeEventSource.emit('tts.audio', {
                    role: 'question',
                    audio: 'AQID',
                    mimeType: 'audio/wav',
                    sequence: 1
                })
            })
            await completeQuestionPlayback({timersMocked: true})

            sendVoiceAudioChunk.mockRejectedValue(new Error('network down'))

            act(() => {
                emitAudioFrame(0.1)
                emitAudioFrame(0)
                emitAudioFrame(0)
                emitAudioFrame(0)
            })
            await act(async () => {
                await Promise.resolve()
            })

            expect(sendVoiceAudioChunk).toHaveBeenCalledTimes(1)

            const retryDelays = [500, 1000, 2000, 4000, 4000]
            for (const delay of retryDelays) {
                await act(async () => {
                    await vi.advanceTimersByTimeAsync(delay)
                })
            }
            await act(async () => {
                await Promise.resolve()
            })

            expect(result.current.sessionActive).toBe(false)
            expect(result.current.speechState).toBe('IDLE')
            expect(stopVoiceSession).not.toHaveBeenCalled()
            expect(onStatus).toHaveBeenCalledWith('오디오 전송 재시도에 실패해 세션을 종료했습니다.')
            unmount()
        } finally {
            vi.useRealTimers()
        }
    })

    it('stops session when captured turn exceeds the max duration', async () => {
        const {result, unmount} = renderHook(() => useVoiceSession({
            sessionId: 's1',
            feedbackModel: 'gpt-4o-mini',
            voiceSttModel: 'gpt-4o-mini-transcribe',
            feedbackLang: 'ko',
            voice: 'alloy',
            onStatus: vi.fn(),
            onFeedback: vi.fn(),
            refreshWrongNotes: vi.fn(async () => {
            }),
            onQuestionPrompt: vi.fn()
        }))

        await act(async () => {
            await result.current.startSession()
        })
        await act(async () => {
            activeEventSource.emit('tts.audio', {
                role: 'question',
                audio: 'AQID',
                mimeType: 'audio/wav',
                sequence: 1
            })
        })
        await completeQuestionPlayback()

        act(() => {
            for (let i = 0; i < 151; i += 1) {
                emitAudioFrame(0.1)
            }
        })

        await waitFor(() => {
            expect(stopVoiceSession).toHaveBeenCalledWith(
                'voice-1',
                {forced: true, reason: 'turn_duration_exceeded'}
            )
        })
        expect(result.current.sessionActive).toBe(false)
        expect(sendVoiceAudioChunk).not.toHaveBeenCalled()
        unmount()
    })

    it('opens an AI-off session and tells the learner no AI is used', async () => {
        const onStatus = vi.fn()
        const {result, unmount} = renderHook(() => useVoiceSession({
            sessionId: 's1',
            feedbackModel: 'claude-opus-5-5',
            feedbackProvider: 'anthropic',
            aiEnabled: false,
            autoReplay: true,
            voiceSttModel: 'gpt-4o-mini-transcribe',
            feedbackLang: 'ko',
            voice: 'alloy',
            onStatus,
            onFeedback: vi.fn(),
            refreshWrongNotes: vi.fn(async () => {
            }),
            onQuestionPrompt: vi.fn()
        }))

        await act(async () => {
            await result.current.startSession()
        })

        expect(openVoiceSession).toHaveBeenCalledWith(expect.objectContaining({aiEnabled: false}))
        expect(result.current.sessionAiEnabled).toBe(false)
        expect(onStatus).toHaveBeenCalledWith('세션을 시작했습니다. AI 없이 진행합니다. 질문을 듣고 답변해 주세요.')
        unmount()
    })

    it('reads the question with the device voice when AI is off, then opens the answer window', async () => {
        const {result, unmount} = renderHook(() => useVoiceSession({
            sessionId: 's1',
            feedbackModel: 'gpt-4o-mini',
            aiEnabled: false,
            autoReplay: false,
            voiceSttModel: 'gpt-4o-mini-transcribe',
            feedbackLang: 'ko',
            voice: 'alloy',
            onStatus: vi.fn(),
            onFeedback: vi.fn(),
            refreshWrongNotes: vi.fn(async () => {
            }),
            onQuestionPrompt: vi.fn()
        }))

        await act(async () => {
            await result.current.startSession()
        })
        const audioCount = audioInstances.length
        await act(async () => {
            activeEventSource.emit('question.prompt', {
                turnId: 1,
                question: {id: 'q-1', text: 'question-1', group: 'travel'},
                selection: {exhausted: false}
            })
            activeEventSource.emit('tts.text', {role: 'question', text: 'question-1', sequence: 1})
        })

        expect(spokenTexts).toEqual(['question-1'])
        expect(result.current.speechState).toBe('PLAYING_QUESTION_TTS')

        await finishDeviceVoiceQuestion()

        expect(spokenTexts).toEqual(['question-1', 'question-1'])
        expect(result.current.speechState).toBe('READY_FOR_ANSWER')
        // No synthesized audio was played: only the device voice spoke.
        expect(audioInstances.length).toBe(audioCount)
        unmount()
    })

    it('keeps going with the question on screen when the speech cannot be synthesized', async () => {
        const onStatus = vi.fn()
        const {result, unmount} = renderLocalSpeechSession({aiEnabled: false, ttsModel: 'gpt-4o-mini-tts', onStatus})

        await act(async () => {
            await result.current.startSession()
        })
        await act(async () => {
            activeEventSource.emit('question.prompt', {
                turnId: 1,
                question: {id: 'q-1', text: 'question-1', group: 'travel'},
                selection: {exhausted: false}
            })
            activeEventSource.emit('tts.text', {role: 'question', text: 'question-1', sequence: 1})
        })
        vi.useFakeTimers()
        try {
            await act(async () => {
                speechCallbacks[speechCallbacks.length - 1].onUnavailable()
            })
            await act(async () => {
                await vi.advanceTimersByTimeAsync(3000)
            })
            await act(async () => {
                speechCallbacks[speechCallbacks.length - 1].onUnavailable()
            })
        } finally {
            vi.useRealTimers()
        }

        expect(result.current.sessionActive).toBe(true)
        expect(result.current.speechState).toBe('READY_FOR_ANSWER')
        expect(onStatus).toHaveBeenCalledWith('이 기기에서 음성을 합성하지 못해 화면으로만 보여 드립니다.')
        unmount()
    })

    it('prepares both models at the start of a Korean-feedback session with the local voice', async () => {
        const onStatus = vi.fn()
        loadLocalKoTts.mockImplementation(async (onProgress) => {
            onProgress({file: 'ko', progress: 42})
        })
        const {result, unmount} = renderLocalSpeechSession({ttsModel: 'local-kokoro', feedbackLang: 'ko', onStatus})

        await act(async () => {
            await result.current.startSession()
        })

        expect(neededLocalTtsModels).toHaveBeenCalledWith('ko')
        expect(loadLocalTts).toHaveBeenCalledTimes(1)
        expect(loadLocalKoTts).toHaveBeenCalledTimes(1)
        // The learner sees which model is downloading.
        expect(onStatus).toHaveBeenCalledWith('기기에서 실행할 한국어 음성 합성 모델을 준비하고 있습니다. 처음 한 번은 약 263MB를 내려받으므로 몇 분 걸릴 수 있습니다.')
        expect(onStatus).toHaveBeenCalledWith('한국어 음성 합성 모델을 내려받는 중입니다. (40%)')
        expect(openVoiceSession).toHaveBeenCalledWith(expect.objectContaining({ttsModel: 'local-kokoro'}))

        await act(async () => {
            activeEventSource.emit('tts.text', {role: 'question', text: 'question-1', sequence: 1})
        })

        // Synthesis starts as soon as the text arrives; each sentence picks its model by language.
        // A Korean-feedback session lets a Korean sentence fetch the Korean model.
        expect(prepareLocalSpeech).toHaveBeenCalledWith('question-1', {allowKoreanModel: true})
        expect(speakLocally).toHaveBeenCalledWith('question-1', expect.not.objectContaining({engine: expect.anything()}))
        expect(speakLocally).toHaveBeenCalledWith('question-1', expect.objectContaining({allowKoreanModel: true}))
        unmount()
    })

    it.each([
        ['English feedback', {ttsModel: 'local-kokoro', feedbackLang: 'en'}],
        ['AI off', {aiEnabled: false, ttsModel: 'gpt-4o-mini-tts', feedbackLang: 'ko'}]
    ])('does not let Korean sentences fetch the Korean model in a session with %s', async (_label, settings) => {
        const {result, unmount} = renderLocalSpeechSession(settings)

        await act(async () => {
            await result.current.startSession()
        })
        await act(async () => {
            activeEventSource.emit('tts.text', {role: 'feedback', text: '좋았습니다.', sequence: 1})
        })

        expect(prepareLocalSpeech).toHaveBeenCalledWith('좋았습니다.', {allowKoreanModel: false})
        expect(speakLocally).toHaveBeenCalledWith('좋았습니다.', expect.objectContaining({allowKoreanModel: false}))
        unmount()
    })

    it('shows the English model download progress with its own label', async () => {
        const onStatus = vi.fn()
        loadLocalTts.mockImplementation(async (onProgress) => {
            onProgress({file: 'en', progress: 75})
        })
        const {result, unmount} = renderLocalSpeechSession({ttsModel: 'local-kokoro', feedbackLang: 'en', onStatus})

        await act(async () => {
            await result.current.startSession()
        })

        expect(onStatus).toHaveBeenCalledWith('기기에서 실행할 영어 음성 합성 모델을 준비하고 있습니다. 처음 한 번은 약 90MB를 내려받으므로 몇 분 걸릴 수 있습니다.')
        expect(onStatus).toHaveBeenCalledWith('영어 음성 합성 모델을 내려받는 중입니다. (70%)')
        unmount()
    })

    it('does not prepare the Korean model when the feedback language is English', async () => {
        const {result, unmount} = renderLocalSpeechSession({ttsModel: 'local-kokoro', feedbackLang: 'en'})

        await act(async () => {
            await result.current.startSession()
        })

        expect(neededLocalTtsModels).toHaveBeenCalledWith('en')
        expect(loadLocalTts).toHaveBeenCalledTimes(1)
        expect(loadLocalKoTts).not.toHaveBeenCalled()
        expect(result.current.sessionActive).toBe(true)
        unmount()
    })

    it('prepares only Kokoro for an AI-off session', async () => {
        const {result, unmount} = renderLocalSpeechSession({aiEnabled: false, ttsModel: 'gpt-4o-mini-tts', feedbackLang: 'ko'})

        await act(async () => {
            await result.current.startSession()
        })

        expect(neededLocalTtsModels).toHaveBeenCalledWith(null)
        expect(loadLocalTts).toHaveBeenCalledTimes(1)
        expect(loadLocalKoTts).not.toHaveBeenCalled()
        expect(result.current.sessionActive).toBe(true)
        unmount()
    })

    it('goes on with the questions after session.ready when the Korean model cannot be prepared', async () => {
        loadLocalKoTts.mockRejectedValue(new Error('HTTP 403'))
        const onStatus = vi.fn()
        const {result, unmount} = renderLocalSpeechSession({ttsModel: 'local-kokoro', feedbackLang: 'ko', onStatus})

        await act(async () => {
            await result.current.startSession()
        })
        expect(onStatus).toHaveBeenCalledWith('한국어 음성 합성 모델을 불러오지 못해 한국어 피드백은 화면으로만 보여 드립니다.')
        expect(result.current.sessionActive).toBe(true)
        await act(async () => {
            activeEventSource.emit('session.ready', {sessionId: 's1', voiceSessionId: 'voice-1'})
        })
        await hearQuestionFromDeviceVoice({turnId: 1, questionId: 'q-1'})

        expect(spokenTexts).toEqual(['question-1', 'question-1'])
        expect(result.current.speechState).toBe('READY_FOR_ANSWER')
        unmount()
    })

    it('tells the learner once that Korean sentences are shown without sound', async () => {
        const onStatus = vi.fn()
        const {result, unmount} = renderLocalSpeechSession({ttsModel: 'local-kokoro', feedbackLang: 'en', onStatus})

        await act(async () => {
            await result.current.startSession()
        })
        await act(async () => {
            activeEventSource.emit('tts.text', {role: 'feedback', text: '좋았습니다.', sequence: 1})
        })
        await act(async () => {
            speechCallbacks[0].onNotice('korean-unavailable')
            speechCallbacks[0].onNotice('korean-unavailable')
        })

        const notices = onStatus.mock.calls.filter(([message]) => message === '한국어 음성 모델이 준비되지 않아 한국어 문장은 화면으로만 보여 드립니다.')
        expect(notices).toHaveLength(1)
        unmount()
    })

    it('still starts an AI-off session when Kokoro cannot be loaded', async () => {
        loadLocalTts.mockRejectedValue(new Error('offline'))
        const {result, unmount} = renderLocalSpeechSession({aiEnabled: false, ttsModel: 'gpt-4o-mini-tts'})

        await act(async () => {
            await result.current.startSession()
        })

        expect(result.current.sessionActive).toBe(true)
        expect(openVoiceSession).toHaveBeenCalled()
        unmount()
    })

    it('does not start the session when the selected local voice cannot load Kokoro', async () => {
        loadLocalTts.mockRejectedValue(new Error('offline'))
        const onStatus = vi.fn()
        const {result, unmount} = renderLocalSpeechSession({ttsModel: 'local-kokoro', onStatus})

        await act(async () => {
            await result.current.startSession()
        })

        expect(openVoiceSession).not.toHaveBeenCalled()
        expect(result.current.sessionActive).toBe(false)
        expect(onStatus).toHaveBeenLastCalledWith(expect.stringContaining('로컬 음성 합성 모델을 불러오지 못했습니다.'))
        unmount()
    })

    it('opens the session once when start is pressed again while the models load', async () => {
        // One shared load, as the real client does for repeated calls.
        let finishLoad
        const load = new Promise((resolve) => {
            finishLoad = resolve
        })
        loadLocalTts.mockImplementation(() => load)
        const {result, unmount} = renderLocalSpeechSession({ttsModel: 'local-kokoro', feedbackLang: 'en'})

        let first
        let second
        await act(async () => {
            first = result.current.startSession()
            second = result.current.startSession()
        })
        expect(result.current.sessionPhase).toBe('STARTING')
        await act(async () => {
            finishLoad()
            await Promise.all([first, second])
        })

        expect(openVoiceSession).toHaveBeenCalledTimes(1)
        expect(eventSourceInstances).toHaveLength(1)
        expect(result.current.sessionActive).toBe(true)
        unmount()
    })

    it('does not load any voice model for hosted TTS', async () => {
        const {result, unmount} = renderLocalSpeechSession({ttsModel: 'gpt-4o-mini-tts'})

        await act(async () => {
            await result.current.startSession()
        })

        expect(neededLocalTtsModels).not.toHaveBeenCalled()
        expect(loadLocalTts).not.toHaveBeenCalled()
        expect(loadLocalKoTts).not.toHaveBeenCalled()
        unmount()
    })

    it('transcribes on the device and sends only the text when local STT is selected', async () => {
        const {result, unmount} = renderHook(() => useVoiceSession({
            sessionId: 's1',
            feedbackModel: 'gpt-4o-mini',
            aiEnabled: true,
            autoReplay: false,
            voiceSttModel: 'local-whisper',
            feedbackLang: 'ko',
            voice: 'alloy',
            onStatus: vi.fn(),
            onFeedback: vi.fn(),
            refreshWrongNotes: vi.fn(async () => {
            }),
            onQuestionPrompt: vi.fn()
        }))

        await act(async () => {
            await result.current.startSession()
        })
        expect(loadLocalStt).toHaveBeenCalledTimes(1)
        expect(openVoiceSession).toHaveBeenCalledWith(expect.objectContaining({sttModel: 'local-whisper'}))

        await hearQuestionFromDeviceVoice({turnId: 1, questionId: 'q-1'})
        speakAnAnswer()
        await waitFor(() => {
            expect(sendVoiceAnswer).toHaveBeenCalledWith('voice-1', {sequence: 1, text: 'local transcript'}, expect.anything())
        })

        expect(transcribePcm16).toHaveBeenCalledTimes(1)
        expect(sendVoiceAudioChunk).not.toHaveBeenCalled()
        unmount()
    })

    it('asks for the answer again when local STT hears nothing', async () => {
        transcribePcm16.mockResolvedValue('   ')
        const onStatus = vi.fn()
        const {result, unmount} = renderHook(() => useVoiceSession({
            sessionId: 's1',
            feedbackModel: 'gpt-4o-mini',
            aiEnabled: true,
            autoReplay: false,
            voiceSttModel: 'local-whisper',
            feedbackLang: 'ko',
            voice: 'alloy',
            onStatus,
            onFeedback: vi.fn(),
            refreshWrongNotes: vi.fn(async () => {
            }),
            onQuestionPrompt: vi.fn()
        }))

        await act(async () => {
            await result.current.startSession()
        })
        await hearQuestionFromDeviceVoice({turnId: 1, questionId: 'q-1'})
        speakAnAnswer()
        await waitFor(() => {
            expect(onStatus).toHaveBeenCalledWith('음성이 인식되지 않아 다시 답변해 주세요.')
        })

        expect(sendVoiceAnswer).not.toHaveBeenCalled()
        expect(result.current.speechState).toBe('READY_FOR_ANSWER')
        unmount()
    })

    it('does not start the session when the local STT model cannot be loaded', async () => {
        loadLocalStt.mockRejectedValue(new Error('offline'))
        const onStatus = vi.fn()
        const {result, unmount} = renderHook(() => useVoiceSession({
            sessionId: 's1',
            feedbackModel: 'gpt-4o-mini',
            aiEnabled: true,
            autoReplay: false,
            voiceSttModel: 'local-whisper',
            feedbackLang: 'ko',
            voice: 'alloy',
            onStatus,
            onFeedback: vi.fn(),
            refreshWrongNotes: vi.fn(async () => {
            }),
            onQuestionPrompt: vi.fn()
        }))

        await act(async () => {
            await result.current.startSession()
        })

        expect(openVoiceSession).not.toHaveBeenCalled()
        expect(result.current.sessionActive).toBe(false)
        expect(onStatus).toHaveBeenLastCalledWith(expect.stringContaining('로컬 음성 인식 모델을 불러오지 못했습니다.'))
        unmount()
    })

    it('reports the end of the answer instead of uploading audio when AI is off', async () => {
        const {result, unmount} = renderHook(() => useVoiceSession({
            sessionId: 's1',
            feedbackModel: 'gpt-4o-mini',
            aiEnabled: false,
            autoReplay: false,
            voiceSttModel: 'gpt-4o-mini-transcribe',
            feedbackLang: 'ko',
            voice: 'alloy',
            onStatus: vi.fn(),
            onFeedback: vi.fn(),
            refreshWrongNotes: vi.fn(async () => {
            }),
            onQuestionPrompt: vi.fn()
        }))

        await act(async () => {
            await result.current.startSession()
        })
        await answerWithoutAi({turnId: 1, questionId: 'q-1'})

        expect(sendVoiceAudioChunk).not.toHaveBeenCalled()
        expect(sendVoiceAnswer).toHaveBeenCalledWith('voice-1', {sequence: 1}, expect.anything())
        expect(result.current.replayPlaying).toBe(false)
        expect(result.current.replayButtonDisabled).toBe(false)
        unmount()
    })

    it('replays the answer automatically when AI is off, then reads the next question', async () => {
        const {result, unmount} = renderHook(() => useVoiceSession({
            sessionId: 's1',
            feedbackModel: 'gpt-4o-mini',
            aiEnabled: false,
            autoReplay: true,
            voiceSttModel: 'gpt-4o-mini-transcribe',
            feedbackLang: 'ko',
            voice: 'alloy',
            onStatus: vi.fn(),
            onFeedback: vi.fn(),
            refreshWrongNotes: vi.fn(async () => {
            }),
            onQuestionPrompt: vi.fn()
        }))

        await act(async () => {
            await result.current.startSession()
        })
        await answerWithoutAi({turnId: 1, questionId: 'q-1'})

        // answer.captured alone started the playback: no click on the replay button.
        expect(result.current.replayPlaying).toBe(true)
        expect(result.current.replayButtonLabel).toBe('중지')
        const replayAudio = audioInstances[audioInstances.length - 1]

        const spokenBeforeNextQuestion = spokenTexts.length
        await act(async () => {
            activeEventSource.emit('question.prompt', {
                turnId: 2,
                question: {id: 'q-2', text: 'question-2', group: 'travel'},
                selection: {exhausted: false}
            })
            activeEventSource.emit('tts.text', {role: 'question', text: 'question-2', sequence: 2})
        })
        // The next question waits: the device voice must not talk over the replay.
        expect(spokenTexts.length).toBe(spokenBeforeNextQuestion)
        expect(result.current.speechState).toBe('WAITING_TTS')

        await act(async () => {
            replayAudio.onended()
        })
        expect(result.current.replayPlaying).toBe(false)
        expect(spokenTexts[spokenTexts.length - 1]).toBe('question-2')
        expect(result.current.speechState).toBe('PLAYING_QUESTION_TTS')
        unmount()
    })

    it('does not replay automatically when AI is on', async () => {
        const {result, unmount} = renderHook(() => useVoiceSession({
            sessionId: 's1',
            feedbackModel: 'gpt-4o-mini',
            aiEnabled: true,
            autoReplay: true,
            voiceSttModel: 'gpt-4o-mini-transcribe',
            feedbackLang: 'ko',
            voice: 'alloy',
            onStatus: vi.fn(),
            onFeedback: vi.fn(),
            refreshWrongNotes: vi.fn(async () => {
            }),
            onQuestionPrompt: vi.fn()
        }))
        await act(async () => {
            await result.current.startSession()
        })
        await prepareConfirmedReplay({turnId: 1, questionId: 'q-1', sequence: 1})

        expect(sendVoiceAnswer).not.toHaveBeenCalled()
        expect(result.current.replayPlaying).toBe(false)
        expect(result.current.awaitingReview).toBe(true)
        unmount()
    })

    it('lets the last answer finish replaying when the server ends the session', async () => {
        const {result, unmount} = renderHook(() => useVoiceSession({
            sessionId: 's1',
            feedbackModel: 'gpt-4o-mini',
            aiEnabled: false,
            autoReplay: true,
            voiceSttModel: 'gpt-4o-mini-transcribe',
            feedbackLang: 'ko',
            voice: 'alloy',
            onStatus: vi.fn(),
            onFeedback: vi.fn(),
            refreshWrongNotes: vi.fn(async () => {
            }),
            onQuestionPrompt: vi.fn()
        }))

        await act(async () => {
            await result.current.startSession()
        })
        await answerWithoutAi({turnId: 1, questionId: 'q-1'})
        expect(result.current.replayPlaying).toBe(true)
        const replayAudio = audioInstances[audioInstances.length - 1]

        await act(async () => {
            activeEventSource.emit('session.stopped', {reason: 'question_exhausted'})
        })

        expect(result.current.sessionActive).toBe(false)
        expect(result.current.replayPlaying).toBe(true)

        await act(async () => {
            replayAudio.onended()
        })
        expect(result.current.replayPlaying).toBe(false)
        expect(result.current.replayButtonDisabled).toBe(false)
        unmount()
    })

    it('stops the replay when the learner ends the session', async () => {
        const {result, unmount} = renderHook(() => useVoiceSession({
            sessionId: 's1',
            feedbackModel: 'gpt-4o-mini',
            aiEnabled: false,
            autoReplay: true,
            voiceSttModel: 'gpt-4o-mini-transcribe',
            feedbackLang: 'ko',
            voice: 'alloy',
            onStatus: vi.fn(),
            onFeedback: vi.fn(),
            refreshWrongNotes: vi.fn(async () => {
            }),
            onQuestionPrompt: vi.fn()
        }))

        await act(async () => {
            await result.current.startSession()
        })
        await answerWithoutAi({turnId: 1, questionId: 'q-1'})
        expect(result.current.replayPlaying).toBe(true)

        await act(async () => {
            await result.current.stopSession()
        })

        expect(result.current.replayPlaying).toBe(false)
        unmount()
    })

    it('keeps confirmed replay audio after session stop', async () => {
        const {result, unmount} = renderHook(() => useVoiceSession({
            sessionId: 's1',
            feedbackModel: 'gpt-4o-mini',
            voiceSttModel: 'gpt-4o-mini-transcribe',
            feedbackLang: 'ko',
            voice: 'alloy',
            onStatus: vi.fn(),
            onFeedback: vi.fn(),
            refreshWrongNotes: vi.fn(async () => {
            }),
            onQuestionPrompt: vi.fn()
        }))

        await act(async () => {
            await result.current.startSession()
        })
        await prepareConfirmedReplay({turnId: 1, questionId: 'q-1', sequence: 1})

        expect(result.current.replayButtonDisabled).toBe(false)
        expect(result.current.replayButtonLabel).toBe('내 응답 재생')

        await act(async () => {
            await result.current.stopSession()
        })
        expect(result.current.sessionActive).toBe(false)
        expect(result.current.replayButtonDisabled).toBe(false)

        await act(async () => {
            result.current.handleReplayAction()
        })
        expect(result.current.replayButtonLabel).toBe('중지')

        await act(async () => {
            audioInstances[audioInstances.length - 1].onended()
        })
        expect(result.current.replayButtonLabel).toBe('내 응답 재생')
        unmount()
    })

    it('clears replay audio when a new session starts', async () => {
        const {result, unmount} = renderHook(() => useVoiceSession({
            sessionId: 's1',
            feedbackModel: 'gpt-4o-mini',
            voiceSttModel: 'gpt-4o-mini-transcribe',
            feedbackLang: 'ko',
            voice: 'alloy',
            onStatus: vi.fn(),
            onFeedback: vi.fn(),
            refreshWrongNotes: vi.fn(async () => {
            }),
            onQuestionPrompt: vi.fn()
        }))

        await act(async () => {
            await result.current.startSession()
        })
        await prepareConfirmedReplay({turnId: 1, questionId: 'q-1', sequence: 1})
        expect(result.current.replayButtonDisabled).toBe(false)

        await act(async () => {
            await result.current.stopSession()
        })
        expect(result.current.replayButtonDisabled).toBe(false)

        await act(async () => {
            await result.current.startSession()
        })
        expect(result.current.replayButtonDisabled).toBe(true)
        expect(result.current.replayButtonDisabledReason).toBe('다시 들을 수 있는 응답이 없습니다.')
        unmount()
    })

    it('does not confirm replay audio when stt.skipped is received', async () => {
        const {result, unmount} = renderHook(() => useVoiceSession({
            sessionId: 's1',
            feedbackModel: 'gpt-4o-mini',
            voiceSttModel: 'gpt-4o-mini-transcribe',
            feedbackLang: 'ko',
            voice: 'alloy',
            onStatus: vi.fn(),
            onFeedback: vi.fn(),
            refreshWrongNotes: vi.fn(async () => {
            }),
            onQuestionPrompt: vi.fn()
        }))

        await act(async () => {
            await result.current.startSession()
        })
        await act(async () => {
            activeEventSource.emit('question.prompt', {
                turnId: 1,
                question: {id: 'q-1', text: 'question-1', group: 'travel'},
                selection: {exhausted: false}
            })
            activeEventSource.emit('tts.audio', {
                role: 'question',
                audio: 'AQID',
                mimeType: 'audio/wav',
                sequence: 1
            })
        })
        await completeQuestionPlayback()

        act(() => {
            emitAudioFrame(0.1)
            emitAudioFrame(0)
            emitAudioFrame(0)
            emitAudioFrame(0)
        })
        await waitFor(() => {
            expect(sendVoiceAudioChunk).toHaveBeenCalledTimes(1)
        })

        await act(async () => {
            activeEventSource.emit('stt.skipped', {turnId: 1, reason: 'empty_transcript'})
        })

        expect(result.current.replayButtonDisabled).toBe(true)
        expect(result.current.replayButtonDisabledReason).toBe('다시 들을 수 있는 응답이 없습니다.')
        unmount()
    })

    it('queues tts audio while replay playback is active and flushes after replay ends', async () => {
        const {result, unmount} = renderHook(() => useVoiceSession({
            sessionId: 's1',
            feedbackModel: 'gpt-4o-mini',
            voiceSttModel: 'gpt-4o-mini-transcribe',
            feedbackLang: 'ko',
            voice: 'alloy',
            onStatus: vi.fn(),
            onFeedback: vi.fn(),
            refreshWrongNotes: vi.fn(async () => {
            }),
            onQuestionPrompt: vi.fn()
        }))

        await act(async () => {
            await result.current.startSession()
        })
        await prepareConfirmedReplay({turnId: 1, questionId: 'q-1', sequence: 1})

        const beforeReplayAudioCount = audioInstances.length
        await act(async () => {
            result.current.handleReplayAction()
        })
        expect(result.current.replayButtonLabel).toBe('중지')
        expect(audioInstances.length).toBe(beforeReplayAudioCount + 1)

        const replayAudio = audioInstances[audioInstances.length - 1]
        await act(async () => {
            activeEventSource.emit('tts.audio', {
                role: 'feedback',
                audio: 'AQID',
                mimeType: 'audio/wav',
                sequence: 2
            })
        })
        expect(audioInstances.length).toBe(beforeReplayAudioCount + 1)

        await act(async () => {
            replayAudio.onended()
        })

        await waitFor(() => {
            expect(audioInstances.length).toBe(beforeReplayAudioCount + 2)
        })
        expect(result.current.speechState).toBe('PLAYING_FEEDBACK_TTS')
        unmount()
    })

    it('blocks microphone capture while replay starts from READY_FOR_ANSWER', async () => {
        const {result, unmount} = renderHook(() => useVoiceSession({
            sessionId: 's1',
            feedbackModel: 'gpt-4o-mini',
            voiceSttModel: 'gpt-4o-mini-transcribe',
            feedbackLang: 'ko',
            voice: 'alloy',
            onStatus: vi.fn(),
            onFeedback: vi.fn(),
            refreshWrongNotes: vi.fn(async () => {
            }),
            onQuestionPrompt: vi.fn()
        }))

        await act(async () => {
            await result.current.startSession()
        })
        await prepareConfirmedReplay({turnId: 1, questionId: 'q-1', sequence: 1})

        await act(async () => {
            activeEventSource.emit('question.prompt', {
                turnId: 2,
                question: {id: 'q-2', text: 'question-2', group: 'travel'},
                selection: {exhausted: false}
            })
            activeEventSource.emit('tts.audio', {
                role: 'question',
                audio: 'AQID',
                mimeType: 'audio/wav',
                sequence: 2
            })
        })
        await completeQuestionPlayback()
        expect(result.current.speechState).toBe('READY_FOR_ANSWER')

        const uploadsBeforeReplay = sendVoiceAudioChunk.mock.calls.length
        await act(async () => {
            result.current.handleReplayAction()
        })
        expect(result.current.replayButtonLabel).toBe('중지')

        act(() => {
            emitAudioFrame(0.1)
        })

        expect(result.current.speechState).toBe('READY_FOR_ANSWER')
        expect(sendVoiceAudioChunk.mock.calls.length).toBe(uploadsBeforeReplay)

        await act(async () => {
            audioInstances[audioInstances.length - 1].onended()
        })
        unmount()
    })
})
