import {useCallback, useEffect, useRef, useState} from 'react'
import {getVoiceEventsUrl, openVoiceSession, sendVoiceAudioChunk, stopVoiceSession} from '../../../shared/api/http.js'
import {float32ToInt16, toBase64} from '../../../shared/utils/audioCodec.js'
import {useAudioDevices} from './useAudioDevices.js'
import {useTtsPlaybackQueue} from './useTtsPlaybackQueue.js'
import {bindVoiceEvents} from './bindVoiceEvents.js'
import {
    AUDIO_PROCESS_BUFFER_SIZE,
    DEFAULT_SAMPLE_RATE,
    MIN_TURN_DURATION_MS,
    NO_WINDOW_ID,
    SPEECH_STATE,
    VAD_RMS_THRESHOLD,
    VAD_SILENCE_MS,
    VOICE_RECONNECT_BASE_DELAY_MS,
    VOICE_RECONNECT_MAX_ATTEMPTS,
    VOICE_RECONNECT_MAX_DELAY_MS
} from './voiceSessionConstants.js'
import {computeRms, joinByteChunks} from './voiceSessionAudio.js'

export function useVoiceSession({
                                    sessionId,
                                    feedbackModel,
                                    voiceSttModel,
                                    ttsModel,
                                    feedbackLang,
                                    voice,
                                    onStatus,
                                    onFeedback,
                                    refreshWrongNotes,
                                    onQuestionPrompt
                                }) {
    const [sessionActive, setSessionActive] = useState(false)
    const [sessionPhase, setSessionPhase] = useState('IDLE')
    const [voiceConnected, setVoiceConnected] = useState(false)
    const [partialTranscript, setPartialTranscript] = useState('')
    const [transcript, setTranscript] = useState('')
    const [userText, setUserText] = useState('')
    const [speechState, setSpeechState] = useState(SPEECH_STATE.IDLE)

    const statusRef = useRef(onStatus)
    const feedbackRef = useRef(onFeedback)
    const refreshWrongNotesRef = useRef(refreshWrongNotes)
    const questionPromptRef = useRef(onQuestionPrompt)

    const settingsRef = useRef({
        feedbackModel,
        voiceSttModel,
        ttsModel,
        feedbackLang,
        voice
    })

    const voiceSessionIdRef = useRef('')
    const eventSourceRef = useRef(null)

    const sessionActiveRef = useRef(false)
    const speechStateRef = useRef(SPEECH_STATE.IDLE)
    const localStopRef = useRef(false)
    const serverStopRef = useRef(false)

    const streamRef = useRef(null)
    const audioContextRef = useRef(null)
    const sourceNodeRef = useRef(null)
    const processorRef = useRef(null)

    const pendingAudioChunksRef = useRef([])
    const pendingAudioSizeRef = useRef(0)
    const chunkSequenceRef = useRef(0)
    const flushInFlightRef = useRef(false)
    const sampleRateRef = useRef(DEFAULT_SAMPLE_RATE)
    const speechActiveRef = useRef(false)
    const silenceDurationMsRef = useRef(0)
    const turnDurationMsRef = useRef(0)
    const answerWindowIdRef = useRef(0)
    const captureWindowIdRef = useRef(NO_WINDOW_ID)
    const stopSessionRef = useRef(null)
    const reconnectAttemptsRef = useRef(0)
    const reconnectTimerRef = useRef(null)
    const reconnectingRef = useRef(false)
    const onConnectionErrorRef = useRef(() => {
    })

    useEffect(() => {
        statusRef.current = onStatus
    }, [onStatus])

    useEffect(() => {
        feedbackRef.current = onFeedback
    }, [onFeedback])

    useEffect(() => {
        refreshWrongNotesRef.current = refreshWrongNotes
    }, [refreshWrongNotes])

    useEffect(() => {
        questionPromptRef.current = onQuestionPrompt
    }, [onQuestionPrompt])

    useEffect(() => {
        settingsRef.current = {
            feedbackModel,
            voiceSttModel,
            ttsModel,
            feedbackLang,
            voice
        }
    }, [feedbackModel, voiceSttModel, ttsModel, feedbackLang, voice])

    const setStatus = useCallback((message) => {
        if (statusRef.current) {
            statusRef.current(message)
        }
    }, [])

    const updateSpeechState = useCallback((nextState) => {
        speechStateRef.current = nextState
        setSpeechState(nextState)
    }, [])

    const resetPendingAudio = useCallback(() => {
        pendingAudioChunksRef.current = []
        pendingAudioSizeRef.current = 0
        flushInFlightRef.current = false
        speechActiveRef.current = false
        silenceDurationMsRef.current = 0
        turnDurationMsRef.current = 0
        captureWindowIdRef.current = NO_WINDOW_ID
    }, [])

    const canCaptureInState = useCallback((state) => {
        return state === SPEECH_STATE.READY_FOR_ANSWER || state === SPEECH_STATE.CAPTURING_ANSWER
    }, [])

    const handleTtsFailure = useCallback((message = 'TTS 출력 실패로 세션을 종료했습니다.') => {
        const stop = stopSessionRef.current
        if (!stop) {
            return
        }
        stop({
            forced: true,
            notifyServer: true,
            reason: 'tts_failed',
            statusMessage: message
        }).catch(() => {
        })
    }, [])

    const onQuestionPlaybackCompleted = useCallback(() => {
        answerWindowIdRef.current += 1
        updateSpeechState(SPEECH_STATE.READY_FOR_ANSWER)
    }, [updateSpeechState])

    const {
        clearTtsPlayback,
        enqueueTtsAudio
    } = useTtsPlaybackQueue({
        resetPendingAudio,
        updateSpeechState,
        onQuestionPlaybackCompleted,
        onTtsFailure: handleTtsFailure,
        speechState: SPEECH_STATE
    })

    const {
        audioPermission,
        setAudioPermission,
        audioDeviceStatus,
        refreshAudioDeviceStatus,
        handleAudioQuickAction
    } = useAudioDevices({
        sessionActiveRef,
        streamRef,
        setStatus
    })

    const stopCapture = useCallback((statusMessage = '') => {
        resetPendingAudio()
        clearTtsPlayback()

        if (processorRef.current) {
            processorRef.current.disconnect()
            processorRef.current.onaudioprocess = null
            processorRef.current = null
        }

        if (sourceNodeRef.current) {
            sourceNodeRef.current.disconnect()
            sourceNodeRef.current = null
        }

        if (audioContextRef.current) {
            audioContextRef.current.close().catch(() => {
            })
            audioContextRef.current = null
        }

        if (streamRef.current) {
            streamRef.current.getTracks().forEach((track) => track.stop())
            streamRef.current = null
        }

        if (statusMessage) {
            setStatus(statusMessage)
        }
        refreshAudioDeviceStatus().catch(() => {
        })
    }, [clearTtsPlayback, refreshAudioDeviceStatus, resetPendingAudio, setStatus])

    const clearReconnectTimer = useCallback(() => {
        if (reconnectTimerRef.current !== null) {
            clearTimeout(reconnectTimerRef.current)
            reconnectTimerRef.current = null
        }
    }, [])

    const resetReconnectState = useCallback(() => {
        clearReconnectTimer()
        reconnectAttemptsRef.current = 0
        reconnectingRef.current = false
    }, [clearReconnectTimer])

    const flushPendingAudio = useCallback(async ({force = false, expectedWindowId = null} = {}) => {
        const voiceSessionId = voiceSessionIdRef.current
        if (!voiceSessionId) {
            return
        }
        if (flushInFlightRef.current && !force) {
            return
        }
        if (!pendingAudioSizeRef.current) {
            return
        }
        if (!canCaptureInState(speechStateRef.current)) {
            resetPendingAudio()
            return
        }
        const capturedWindowId = expectedWindowId ?? captureWindowIdRef.current
        if (capturedWindowId === NO_WINDOW_ID || capturedWindowId !== answerWindowIdRef.current) {
            resetPendingAudio()
            return
        }

        const chunks = pendingAudioChunksRef.current
        const totalSize = pendingAudioSizeRef.current
        const durationMs = turnDurationMsRef.current
        const sampleRate = sampleRateRef.current || DEFAULT_SAMPLE_RATE

        pendingAudioChunksRef.current = []
        pendingAudioSizeRef.current = 0
        speechActiveRef.current = false
        silenceDurationMsRef.current = 0
        turnDurationMsRef.current = 0

        if (durationMs < MIN_TURN_DURATION_MS) {
            return
        }

        updateSpeechState(SPEECH_STATE.WAITING_TTS)
        const merged = joinByteChunks(chunks, totalSize)
        flushInFlightRef.current = true
        try {
            if (capturedWindowId !== answerWindowIdRef.current) {
                return
            }
            await sendVoiceAudioChunk(voiceSessionId, {
                pcm16Base64: toBase64(merged),
                sampleRate,
                sequence: ++chunkSequenceRef.current
            })
        } catch (_error) {
            setStatus('오디오 조각 전송에 실패했습니다.')
        } finally {
            flushInFlightRef.current = false
            if (pendingAudioSizeRef.current > 0) {
                setTimeout(() => {
                    flushPendingAudio({force: true, expectedWindowId: capturedWindowId}).catch(() => {
                    })
                }, 0)
            }
        }
    }, [canCaptureInState, resetPendingAudio, setStatus, updateSpeechState])

    const closeEventSource = useCallback(() => {
        if (eventSourceRef.current) {
            eventSourceRef.current.close()
            eventSourceRef.current = null
        }
    }, [])

    const stopSession = useCallback(async ({
                                               forced = true,
                                               notifyServer = true,
                                               reason = 'user_stop',
                                               statusMessage = '세션을 종료했습니다.'
                                           } = {}) => {
        if (!sessionActiveRef.current && !voiceSessionIdRef.current && !eventSourceRef.current) {
            return
        }

        localStopRef.current = notifyServer
        resetReconnectState()
        setSessionPhase('STOPPING')
        updateSpeechState(SPEECH_STATE.STOPPING)
        resetPendingAudio()
        sessionActiveRef.current = false
        setSessionActive(false)

        await flushPendingAudio({force: true})
        stopCapture('')

        const voiceSessionId = voiceSessionIdRef.current
        if (notifyServer && voiceSessionId) {
            await stopVoiceSession(voiceSessionId, {forced, reason}).catch(() => {
            })
        }

        closeEventSource()
        voiceSessionIdRef.current = ''
        setVoiceConnected(false)
        setSessionPhase('IDLE')
        updateSpeechState(SPEECH_STATE.IDLE)

        if (statusMessage) {
            setStatus(statusMessage)
        }
    }, [closeEventSource, flushPendingAudio, resetPendingAudio, resetReconnectState, setStatus, stopCapture, updateSpeechState])

    useEffect(() => {
        stopSessionRef.current = stopSession
    }, [stopSession])

    const bindSessionEvents = useCallback((eventSource) => {
        const refreshWrongNotesAction = () => {
            if (!refreshWrongNotesRef.current) {
                return Promise.resolve()
            }
            return refreshWrongNotesRef.current()
        }

        bindVoiceEvents({
            eventSource,
            serverStopRef,
            setVoiceConnected: (connected) => {
                setVoiceConnected(connected)
                if (connected) {
                    resetReconnectState()
                }
            },
            setStatus,
            setPartialTranscript,
            setTranscript,
            setUserText,
            resetPendingAudio,
            updateSpeechState,
            speechState: SPEECH_STATE,
            onQuestionPrompt: (question) => {
                questionPromptRef.current?.(question)
            },
            onFeedback: (feedback) => {
                feedbackRef.current?.(feedback)
            },
            refreshWrongNotes: refreshWrongNotesAction,
            stopSession,
            enqueueTtsAudio,
            handleTtsFailure,
            onConnectionError: () => {
                onConnectionErrorRef.current?.()
            }
        })
    }, [enqueueTtsAudio, handleTtsFailure, resetPendingAudio, resetReconnectState, setStatus, stopSession, updateSpeechState])

    const scheduleReconnect = useCallback(() => {
        if (!sessionActiveRef.current || !voiceSessionIdRef.current) {
            reconnectingRef.current = false
            return
        }
        if (reconnectingRef.current || reconnectTimerRef.current !== null) {
            return
        }

        const nextAttempt = reconnectAttemptsRef.current + 1
        if (nextAttempt > VOICE_RECONNECT_MAX_ATTEMPTS) {
            reconnectingRef.current = false
            stopSession({
                forced: false,
                notifyServer: false,
                statusMessage: '음성 이벤트 연결이 반복적으로 끊겨 세션을 종료했습니다. 다시 시작해 주세요.'
            }).catch(() => {
            })
            return
        }

        reconnectAttemptsRef.current = nextAttempt
        reconnectingRef.current = true
        setVoiceConnected(false)
        setStatus(`음성 이벤트 연결이 끊겼습니다. 재연결을 시도합니다. (${nextAttempt}/${VOICE_RECONNECT_MAX_ATTEMPTS})`)

        const delayMs = Math.min(
            VOICE_RECONNECT_BASE_DELAY_MS * (2 ** (nextAttempt - 1)),
            VOICE_RECONNECT_MAX_DELAY_MS
        )

        reconnectTimerRef.current = setTimeout(() => {
            reconnectTimerRef.current = null
            const reconnect = async () => {
                if (!sessionActiveRef.current || localStopRef.current || serverStopRef.current) {
                    reconnectingRef.current = false
                    return
                }
                const voiceSessionId = voiceSessionIdRef.current
                if (!voiceSessionId) {
                    reconnectingRef.current = false
                    return
                }
                try {
                    const eventsUrl = await getVoiceEventsUrl(voiceSessionId)
                    closeEventSource()
                    const eventSource = new EventSource(eventsUrl)
                    eventSourceRef.current = eventSource
                    bindSessionEvents(eventSource)
                    reconnectingRef.current = false
                    reconnectAttemptsRef.current = 0
                    setVoiceConnected(true)
                    setStatus('음성 이벤트 연결이 복구되었습니다.')
                } catch (_error) {
                    reconnectingRef.current = false
                    scheduleReconnect()
                }
            }
            reconnect().catch(() => {
                reconnectingRef.current = false
                scheduleReconnect()
            })
        }, delayMs)
    }, [bindSessionEvents, closeEventSource, setStatus, stopSession])

    const handleConnectionError = useCallback(() => {
        if (localStopRef.current || serverStopRef.current) {
            localStopRef.current = false
            serverStopRef.current = false
            return
        }
        if (!sessionActiveRef.current || !voiceSessionIdRef.current) {
            return
        }
        closeEventSource()
        scheduleReconnect()
    }, [closeEventSource, scheduleReconnect])

    useEffect(() => {
        onConnectionErrorRef.current = handleConnectionError
    }, [handleConnectionError])

    const startSession = useCallback(async () => {
        if (sessionActiveRef.current) {
            return
        }

        if (!navigator.mediaDevices || !navigator.mediaDevices.getUserMedia) {
            setAudioPermission('unsupported')
            setStatus('이 환경에서는 오디오 입력 장치를 지원하지 않습니다.')
            return
        }

        try {
            setSessionPhase('STARTING')
            localStopRef.current = false
            serverStopRef.current = false
            resetReconnectState()
            answerWindowIdRef.current = 0
            captureWindowIdRef.current = NO_WINDOW_ID
            updateSpeechState(SPEECH_STATE.WAITING_TTS)

            const created = await openVoiceSession({
                sessionId,
                feedbackModel: settingsRef.current.feedbackModel,
                feedbackLanguage: settingsRef.current.feedbackLang,
                sttModel: settingsRef.current.voiceSttModel,
                ttsModel: settingsRef.current.ttsModel,
                ttsVoice: settingsRef.current.voice
            })
            const voiceSessionId = created?.voiceSessionId
            if (!voiceSessionId) {
                throw new Error('음성 세션 ID를 받지 못했습니다.')
            }
            voiceSessionIdRef.current = voiceSessionId

            const eventsUrl = await getVoiceEventsUrl(voiceSessionId)
            const eventSource = new EventSource(eventsUrl)
            eventSourceRef.current = eventSource
            bindSessionEvents(eventSource)

            const stream = await navigator.mediaDevices.getUserMedia({audio: true})
            const AudioContextClass = window.AudioContext || window.webkitAudioContext
            const audioContext = new AudioContextClass({sampleRate: DEFAULT_SAMPLE_RATE})
            const sourceNode = audioContext.createMediaStreamSource(stream)
            const processor = audioContext.createScriptProcessor(AUDIO_PROCESS_BUFFER_SIZE, 1, 1)
            sampleRateRef.current = audioContext.sampleRate || DEFAULT_SAMPLE_RATE

            processor.onaudioprocess = (audioEvent) => {
                if (!sessionActiveRef.current) {
                    return
                }
                const currentState = speechStateRef.current
                if (!canCaptureInState(currentState)) {
                    resetPendingAudio()
                    return
                }

                const channel = audioEvent.inputBuffer.getChannelData(0)
                const sampleRate = sampleRateRef.current || DEFAULT_SAMPLE_RATE
                const frameDurationMs = (channel.length / sampleRate) * 1000
                const rms = computeRms(channel)
                const pcm = float32ToInt16(channel)
                const bytes = new Uint8Array(pcm.buffer)

                if (rms >= VAD_RMS_THRESHOLD) {
                    if (currentState === SPEECH_STATE.READY_FOR_ANSWER) {
                        captureWindowIdRef.current = answerWindowIdRef.current
                        updateSpeechState(SPEECH_STATE.CAPTURING_ANSWER)
                    }
                    speechActiveRef.current = true
                    silenceDurationMsRef.current = 0
                    pendingAudioChunksRef.current.push(bytes)
                    pendingAudioSizeRef.current += bytes.length
                    turnDurationMsRef.current += frameDurationMs
                    return
                }

                if (!speechActiveRef.current) {
                    return
                }

                silenceDurationMsRef.current += frameDurationMs
                pendingAudioChunksRef.current.push(bytes)
                pendingAudioSizeRef.current += bytes.length
                turnDurationMsRef.current += frameDurationMs
                if (silenceDurationMsRef.current >= VAD_SILENCE_MS) {
                    flushPendingAudio({
                        force: true,
                        expectedWindowId: captureWindowIdRef.current
                    }).catch(() => {
                    })
                }
            }

            sourceNode.connect(processor)
            processor.connect(audioContext.destination)

            streamRef.current = stream
            audioContextRef.current = audioContext
            sourceNodeRef.current = sourceNode
            processorRef.current = processor

            setSessionActive(true)
            sessionActiveRef.current = true
            setSessionPhase('ACTIVE')
            setAudioPermission('granted')
            setStatus('세션을 시작했습니다. 질문이 도착하면 답변해 주세요.')
            refreshAudioDeviceStatus().catch(() => {
            })
        } catch (error) {
            const openedVoiceSessionId = voiceSessionIdRef.current
            setSessionPhase('IDLE')
            updateSpeechState(SPEECH_STATE.IDLE)
            resetReconnectState()
            if (error?.name === 'NotAllowedError' || error?.name === 'SecurityError') {
                setAudioPermission('denied')
                setStatus('마이크 권한이 거부되었습니다. 아래에서 권한 요청을 다시 실행해 주세요.')
            } else if (error?.name === 'NotFoundError') {
                setStatus('사용 가능한 마이크 장치를 찾지 못했습니다.')
            } else {
                setStatus(error?.message || '세션을 시작하지 못했습니다.')
            }
            closeEventSource()
            voiceSessionIdRef.current = ''
            sessionActiveRef.current = false
            setSessionActive(false)
            setVoiceConnected(false)
            stopCapture('')
            if (openedVoiceSessionId) {
                stopVoiceSession(openedVoiceSessionId, {
                    forced: true,
                    reason: 'start_failed'
                }).catch(() => {
                })
            }
            refreshAudioDeviceStatus().catch(() => {
            })
        }
    }, [
        bindSessionEvents,
        canCaptureInState,
        closeEventSource,
        flushPendingAudio,
        refreshAudioDeviceStatus,
        resetPendingAudio,
        resetReconnectState,
        sessionId,
        setAudioPermission,
        setStatus,
        stopCapture,
        updateSpeechState
    ])

    const syncVoiceSettings = useCallback(async () => {
        setStatus('설정은 다음 세션 시작 시 반영됩니다.')
    }, [setStatus])

    useEffect(() => {
        return () => {
            stopSession({forced: false, notifyServer: false, statusMessage: ''}).catch(() => {
            })
        }
    }, [stopSession])

    return {
        sessionActive,
        sessionPhase,
        voiceConnected,
        speechState,
        partialTranscript,
        transcript,
        userText,
        audioPermission,
        audioDeviceStatus,
        startSession,
        stopSession,
        syncVoiceSettings,
        handleAudioQuickAction
    }
}
