/** @vitest-environment jsdom */
import {act, renderHook, waitFor} from '@testing-library/react'
import {beforeEach, describe, expect, it, vi} from 'vitest'
import {useVoiceSession} from './useVoiceSession.js'
import {getVoiceEventsUrl, openVoiceSession, sendVoiceAudioChunk, stopVoiceSession} from '../../../shared/api/http.js'

vi.mock('../../../shared/api/http.js', () => ({
    openVoiceSession: vi.fn(),
    getVoiceEventsUrl: vi.fn(),
    sendVoiceAudioChunk: vi.fn(),
    stopVoiceSession: vi.fn()
}))

let activeEventSource = null
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
    constructor() {
        this.onended = null
        this.onerror = null
        this.src = ''
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
        sendVoiceAudioChunk.mockResolvedValue(undefined)
        stopVoiceSession.mockResolvedValue(undefined)
    })

    it('sends selected models when opening voice session', async () => {
        const {result, unmount} = renderHook(() => useVoiceSession({
            sessionId: 's1',
            feedbackModel: 'gpt-5',
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
            feedbackModel: 'gpt-5',
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

        await act(async () => {
            const questionAudio = audioInstances[audioInstances.length - 1]
            questionAudio.onended()
        })

        expect(result.current.speechState).toBe('READY_FOR_ANSWER')

        act(() => {
            emitAudioFrame(0.1)
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
            })
        )
        unmount()
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
        await act(async () => {
            const questionAudio = audioInstances[audioInstances.length - 1]
            questionAudio.onended()
        })
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
        await act(async () => {
            const questionAudio = audioInstances[audioInstances.length - 1]
            questionAudio.onended()
        })
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
        await act(async () => {
            audioInstances[audioInstances.length - 1].onended()
        })
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
        await act(async () => {
            audioInstances[audioInstances.length - 1].onended()
        })
        expect(result.current.speechState).toBe('READY_FOR_ANSWER')

        // Speak for question 2 only.
        act(() => {
            emitAudioFrame(0.1)
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
            await act(async () => {
                audioInstances[audioInstances.length - 1].onended()
            })

            sendVoiceAudioChunk
                .mockRejectedValueOnce(new Error('temporary'))
                .mockResolvedValueOnce(undefined)

            act(() => {
                emitAudioFrame(0.1)
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
            await act(async () => {
                audioInstances[audioInstances.length - 1].onended()
            })

            sendVoiceAudioChunk.mockRejectedValue(new Error('network down'))

            act(() => {
                emitAudioFrame(0.1)
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
        await act(async () => {
            audioInstances[audioInstances.length - 1].onended()
        })

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
})
