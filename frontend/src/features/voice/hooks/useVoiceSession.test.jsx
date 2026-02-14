/** @vitest-environment jsdom */
import {act, renderHook, waitFor} from '@testing-library/react'
import {beforeEach, describe, expect, it, vi} from 'vitest'
import {useVoiceSession} from './useVoiceSession.js'
import {
    getVoiceEventsUrl,
    openVoiceSession,
    sendVoiceAudioChunk,
    stopVoiceSession
} from '../../../shared/api/http.js'

vi.mock('../../../shared/api/http.js', () => ({
    openVoiceSession: vi.fn(),
    getVoiceEventsUrl: vi.fn(),
    sendVoiceAudioChunk: vi.fn(),
    stopVoiceSession: vi.fn()
}))

let activeEventSource = null
let lastProcessor = null
let audioInstances = []
let audioPlayShouldFail = false

class MockEventSource {
    constructor(url) {
        this.url = url
        this.listeners = {}
        this.onerror = null
        activeEventSource = this
    }

    addEventListener(type, listener) {
        if (!this.listeners[type]) {
            this.listeners[type] = []
        }
        this.listeners[type].push(listener)
    }

    close() {
    }

    emit(type, payload) {
        const event = {
            data: payload === undefined ? '' : JSON.stringify(payload)
        }
        const listeners = this.listeners[type] || []
        listeners.forEach((listener) => listener(event))
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
})
