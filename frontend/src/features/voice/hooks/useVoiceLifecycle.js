import {useCallback, useEffect, useRef, useState} from 'react'
import {
    getVoiceEventsUrl,
    openVoiceSession,
    recoverVoiceSession,
    sendVoiceAudioChunk,
    stopVoiceSession
} from '../../../shared/api/http.js'
import {float32ToInt16, toBase64} from '../../../shared/utils/audioCodec.js'
import {useAudioDevices} from './useAudioDevices.js'
import {useTtsPlaybackQueue} from './useTtsPlaybackQueue.js'
import {bindVoiceEvents} from './bindVoiceEvents.js'
import {
    AUDIO_PROCESS_BUFFER_SIZE,
    DEFAULT_SAMPLE_RATE,
    MAX_TURN_DURATION_MS,
    MIN_TURN_DURATION_MS,
    NO_WINDOW_ID,
    SPEECH_STATE,
    VAD_RMS_THRESHOLD,
    VAD_SILENCE_MS,
    VOICE_CHUNK_RETRY_BASE_DELAY_MS,
    VOICE_CHUNK_RETRY_MAX_DELAY_MS,
    VOICE_CHUNK_UPLOAD_MAX_RETRIES,
    VOICE_RECONNECT_BASE_DELAY_MS,
    VOICE_RECONNECT_MAX_ATTEMPTS,
    VOICE_RECONNECT_MAX_DELAY_MS
} from './voiceSessionConstants.js'
import {computeRms, joinByteChunks} from './voiceSessionAudio.js'

const VOICE_CHUNK_UPLOAD_TIMEOUT_MS = 3000

export function useVoiceLifecycle({
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
    const inFlightTurnRef = useRef(null)
    const chunkRetryTimerRef = useRef(null)
    const chunkUploadAbortRef = useRef(null)
    const chunkUploadTimeoutRef = useRef(null)
    const chunkUploadRequestIdRef = useRef(0)
    const flushPendingAudioRef = useRef(null)
    const sampleRateRef = useRef(DEFAULT_SAMPLE_RATE)
    const speechActiveRef = useRef(false)
    const silenceDurationMsRef = useRef(0)
    const turnDurationMsRef = useRef(0)
    const answerWindowIdRef = useRef(0)
    const captureWindowIdRef = useRef(NO_WINDOW_ID)
    const currentQuestionIdRef = useRef('')
    const captureQuestionIdRef = useRef('')
    const stopSessionRef = useRef(null)
    const reconnectAttemptsRef = useRef(0)
    const reconnectTimerRef = useRef(null)
    const reconnectingRef = useRef(false)
    const lastSeenEventIdRef = useRef(0)
    const replayTtsCutoffEventIdRef = useRef(0)
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

    const parseEventId = useCallback((data) => {
        const raw = data?.eventId
        const parsed = Number(raw)
        if (!Number.isFinite(parsed)) {
            return null
        }
        return Math.max(0, Math.trunc(parsed))
    }, [])

    const shouldProcessEvent = useCallback((data) => {
        const eventId = parseEventId(data)
        if (eventId === null) {
            return true
        }
        if (eventId <= lastSeenEventIdRef.current) {
            return false
        }
        lastSeenEventIdRef.current = eventId
        return true
    }, [parseEventId])

    const shouldSkipReplayTts = useCallback((data) => {
        const cutoff = replayTtsCutoffEventIdRef.current
        if (cutoff <= 0) {
            return false
        }
        const eventId = parseEventId(data)
        if (eventId === null) {
            return false
        }
        return eventId <= cutoff
    }, [parseEventId])

    const shouldResumeImmediatelyOnQuestionPrompt = useCallback((data) => {
        const cutoff = replayTtsCutoffEventIdRef.current
        if (cutoff <= 0) {
            return false
        }
        const eventId = parseEventId(data)
        if (eventId === null) {
            return false
        }
        return eventId <= cutoff
    }, [parseEventId])

    const resetPendingAudio = useCallback(() => {
        pendingAudioChunksRef.current = []
        pendingAudioSizeRef.current = 0
        speechActiveRef.current = false
        silenceDurationMsRef.current = 0
        turnDurationMsRef.current = 0
        captureWindowIdRef.current = NO_WINDOW_ID
        captureQuestionIdRef.current = ''
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

    const clearChunkRetryTimer = useCallback(() => {
        if (chunkRetryTimerRef.current !== null) {
            clearTimeout(chunkRetryTimerRef.current)
            chunkRetryTimerRef.current = null
        }
    }, [])

    const clearChunkUploadTimeout = useCallback(() => {
        if (chunkUploadTimeoutRef.current !== null) {
            clearTimeout(chunkUploadTimeoutRef.current)
            chunkUploadTimeoutRef.current = null
        }
    }, [])

    const abortInFlightChunkUpload = useCallback(() => {
        clearChunkUploadTimeout()
        const abortController = chunkUploadAbortRef.current
        if (!abortController) {
            return
        }
        chunkUploadAbortRef.current = null
        abortController.abort()
    }, [clearChunkUploadTimeout])

    const clearInFlightTurn = useCallback(() => {
        clearChunkRetryTimer()
        abortInFlightChunkUpload()
        inFlightTurnRef.current = null
        flushInFlightRef.current = false
    }, [abortInFlightChunkUpload, clearChunkRetryTimer])

    const stopCapture = useCallback((statusMessage = '') => {
        resetPendingAudio()
        clearInFlightTurn()
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
    }, [clearInFlightTurn, clearTtsPlayback, refreshAudioDeviceStatus, resetPendingAudio, setStatus])

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

    const enterRecoveringState = useCallback(() => {
        clearChunkRetryTimer()
        clearTtsPlayback()
        updateSpeechState(SPEECH_STATE.RECOVERING)
    }, [clearChunkRetryTimer, clearTtsPlayback, updateSpeechState])

    const applyRecoverySnapshot = useCallback((snapshot) => {
        const questionNode = snapshot?.currentQuestion
        currentQuestionIdRef.current = questionNode?.id || ''
        if (questionNode) {
            questionPromptRef.current?.({
                questionId: questionNode.id || '',
                text: questionNode.text || '',
                group: questionNode.group || '',
                exhausted: false,
                selectionReason: ''
            })
            updateSpeechState(SPEECH_STATE.READY_FOR_ANSWER)
            return
        }
        questionPromptRef.current?.(null)
        updateSpeechState(SPEECH_STATE.WAITING_TTS)
    }, [updateSpeechState])

    const handleSttSkipped = useCallback(() => {
        if (!sessionActiveRef.current || !voiceSessionIdRef.current || localStopRef.current || serverStopRef.current) {
            return
        }
        resetPendingAudio()
        updateSpeechState(SPEECH_STATE.READY_FOR_ANSWER)
    }, [resetPendingAudio, updateSpeechState])

    const stopDueToChunkUploadFailure = useCallback((message = '오디오 전송 재시도에 실패해 세션을 종료했습니다.') => {
        const stop = stopSessionRef.current
        if (!stop) {
            setStatus(message)
            return
        }
        stop({
            forced: false,
            notifyServer: false,
            reason: 'audio_chunk_upload_failed',
            statusMessage: message
        }).catch(() => {
        })
    }, [setStatus])

    const uploadInFlightTurn = useCallback(async () => {
        const turn = inFlightTurnRef.current
        if (!turn) {
            return {ok: true, uploadedTurn: null}
        }
        if (flushInFlightRef.current) {
            return {ok: false, uploadedTurn: turn}
        }

        flushInFlightRef.current = true
        const requestId = chunkUploadRequestIdRef.current + 1
        chunkUploadRequestIdRef.current = requestId
        const abortController = new AbortController()
        chunkUploadAbortRef.current = abortController
        clearChunkUploadTimeout()
        chunkUploadTimeoutRef.current = setTimeout(() => {
            if (chunkUploadRequestIdRef.current !== requestId) {
                return
            }
            abortController.abort()
        }, VOICE_CHUNK_UPLOAD_TIMEOUT_MS)
        try {
            await sendVoiceAudioChunk(turn.voiceSessionId, {
                pcm16Base64: turn.pcm16Base64,
                sampleRate: turn.sampleRate,
                sequence: turn.sequence
            }, {
                signal: abortController.signal
            })
            if (inFlightTurnRef.current?.sequence === turn.sequence) {
                inFlightTurnRef.current = null
            }
            clearChunkRetryTimer()
            return {ok: true, uploadedTurn: turn}
        } catch (error) {
            if (error?.name === 'AbortError'
                && sessionActiveRef.current
                && !localStopRef.current
                && !serverStopRef.current) {
                setStatus('오디오 전송이 지연되어 재시도합니다.')
            }
            return {ok: false, uploadedTurn: turn}
        } finally {
            if (chunkUploadRequestIdRef.current === requestId) {
                clearChunkUploadTimeout()
                if (chunkUploadAbortRef.current === abortController) {
                    chunkUploadAbortRef.current = null
                }
            }
            flushInFlightRef.current = false
        }
    }, [clearChunkRetryTimer, clearChunkUploadTimeout, setStatus])

    const scheduleChunkRetry = useCallback(() => {
        const turn = inFlightTurnRef.current
        if (!turn || chunkRetryTimerRef.current !== null) {
            return
        }

        if (turn.retryCount >= VOICE_CHUNK_UPLOAD_MAX_RETRIES) {
            clearInFlightTurn()
            stopDueToChunkUploadFailure()
            return
        }

        turn.retryCount += 1
        setStatus(`오디오 조각 전송 재시도 중입니다. (${turn.retryCount}/${VOICE_CHUNK_UPLOAD_MAX_RETRIES})`)
        const delayMs = Math.min(
            VOICE_CHUNK_RETRY_BASE_DELAY_MS * (2 ** (turn.retryCount - 1)),
            VOICE_CHUNK_RETRY_MAX_DELAY_MS
        )

        chunkRetryTimerRef.current = setTimeout(() => {
            chunkRetryTimerRef.current = null
            if (!sessionActiveRef.current
                || !voiceSessionIdRef.current
                || localStopRef.current
                || serverStopRef.current
                || reconnectingRef.current) {
                return
            }
            uploadInFlightTurn()
                .then(({ok, uploadedTurn}) => {
                    if (!ok) {
                        scheduleChunkRetry()
                        return
                    }
                    if (pendingAudioSizeRef.current > 0 && flushPendingAudioRef.current) {
                        flushPendingAudioRef.current({
                            force: true,
                            expectedWindowId: uploadedTurn?.capturedWindowId ?? null
                        }).catch(() => {
                        })
                    }
                })
                .catch(() => {
                    scheduleChunkRetry()
                })
        }, delayMs)
    }, [clearInFlightTurn, setStatus, stopDueToChunkUploadFailure, uploadInFlightTurn])

    const resumeBufferedAudioAfterRecovery = useCallback(async (snapshot) => {
        const hasInFlight = Boolean(inFlightTurnRef.current)
        const hasPending = pendingAudioSizeRef.current > 0
        if (!hasInFlight && !hasPending) {
            return {droppedDueToMismatch: false}
        }

        const snapshotQuestionId = snapshot?.currentQuestion?.id || ''
        const localQuestionId = inFlightTurnRef.current?.questionId || captureQuestionIdRef.current || ''
        if (!snapshotQuestionId || !localQuestionId || snapshotQuestionId !== localQuestionId) {
            resetPendingAudio()
            clearInFlightTurn()
            return {droppedDueToMismatch: true}
        }

        if (inFlightTurnRef.current) {
            const lastAccepted = Number(snapshot?.lastAcceptedChunkSequence)
            if (Number.isFinite(lastAccepted) && lastAccepted >= inFlightTurnRef.current.sequence) {
                clearInFlightTurn()
            } else {
                if (flushInFlightRef.current) {
                    await Promise.resolve()
                }
                const {ok, uploadedTurn} = await uploadInFlightTurn()
                if (!ok) {
                    scheduleChunkRetry()
                    return {droppedDueToMismatch: false}
                }
                if (pendingAudioSizeRef.current > 0 && flushPendingAudioRef.current) {
                    await flushPendingAudioRef.current({
                        force: true,
                        expectedWindowId: uploadedTurn?.capturedWindowId ?? null
                    })
                }
                return {droppedDueToMismatch: false}
            }
        }

        if (pendingAudioSizeRef.current > 0 && flushPendingAudioRef.current) {
            await flushPendingAudioRef.current({
                force: true,
                expectedWindowId: captureWindowIdRef.current
            })
        }
        return {droppedDueToMismatch: false}
    }, [clearInFlightTurn, resetPendingAudio, scheduleChunkRetry, uploadInFlightTurn])

    const flushPendingAudio = useCallback(async ({force = false, expectedWindowId = null} = {}) => {
        if (inFlightTurnRef.current) {
            return
        }
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
        const capturedQuestionId = captureQuestionIdRef.current || currentQuestionIdRef.current || ''

        if (durationMs < MIN_TURN_DURATION_MS) {
            resetPendingAudio()
            return
        }
        if (durationMs > MAX_TURN_DURATION_MS) {
            resetPendingAudio()
            const stop = stopSessionRef.current
            if (!stop) {
                setStatus('답변 길이가 너무 길어 세션을 종료했습니다. (최대 2분 30초)')
                return
            }
            stop({
                forced: true,
                notifyServer: true,
                reason: 'turn_duration_exceeded',
                statusMessage: '답변 길이가 너무 길어 세션을 종료했습니다. (최대 2분 30초)'
            }).catch(() => {
            })
            return
        }

        const merged = joinByteChunks(chunks, totalSize)
        const nextSequence = ++chunkSequenceRef.current

        pendingAudioChunksRef.current = []
        pendingAudioSizeRef.current = 0
        speechActiveRef.current = false
        silenceDurationMsRef.current = 0
        turnDurationMsRef.current = 0
        captureQuestionIdRef.current = ''

        updateSpeechState(SPEECH_STATE.WAITING_TTS)
        inFlightTurnRef.current = {
            voiceSessionId,
            pcm16Base64: toBase64(merged),
            sampleRate,
            sequence: nextSequence,
            capturedWindowId,
            questionId: capturedQuestionId,
            retryCount: 0
        }

        const {ok, uploadedTurn} = await uploadInFlightTurn()
        if (!ok) {
            if (!reconnectingRef.current && eventSourceRef.current) {
                scheduleChunkRetry()
            }
            return
        }
        if (pendingAudioSizeRef.current > 0 && flushPendingAudioRef.current) {
            setTimeout(() => {
                flushPendingAudioRef.current({
                    force: true,
                    expectedWindowId: uploadedTurn?.capturedWindowId ?? null
                }).catch(() => {
                })
            }, 0)
        }
    }, [canCaptureInState, scheduleChunkRetry, setStatus, resetPendingAudio, updateSpeechState, uploadInFlightTurn])

    useEffect(() => {
        flushPendingAudioRef.current = flushPendingAudio
    }, [flushPendingAudio])

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
        stopCapture('')

        const voiceSessionId = voiceSessionIdRef.current
        if (notifyServer && voiceSessionId) {
            await stopVoiceSession(voiceSessionId, {forced, reason}).catch(() => {
            })
        }

        closeEventSource()
        voiceSessionIdRef.current = ''
        currentQuestionIdRef.current = ''
        lastSeenEventIdRef.current = 0
        replayTtsCutoffEventIdRef.current = 0
        setVoiceConnected(false)
        setSessionPhase('IDLE')
        updateSpeechState(SPEECH_STATE.IDLE)

        if (statusMessage) {
            setStatus(statusMessage)
        }
    }, [closeEventSource, resetPendingAudio, resetReconnectState, setStatus, stopCapture, updateSpeechState])

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
                currentQuestionIdRef.current = question?.questionId || ''
                questionPromptRef.current?.(question)
            },
            onFeedback: (feedback) => {
                feedbackRef.current?.(feedback)
            },
            refreshWrongNotes: refreshWrongNotesAction,
            stopSession,
            shouldResumeImmediatelyOnQuestionPrompt,
            enqueueTtsAudio,
            handleTtsFailure,
            onSttSkipped: handleSttSkipped,
            shouldProcessEvent,
            shouldSkipReplayTts,
            onConnectionError: () => {
                onConnectionErrorRef.current?.()
            }
        })
    }, [
        enqueueTtsAudio,
        handleSttSkipped,
        handleTtsFailure,
        resetPendingAudio,
        resetReconnectState,
        setStatus,
        shouldProcessEvent,
        shouldResumeImmediatelyOnQuestionPrompt,
        shouldSkipReplayTts,
        stopSession,
        updateSpeechState
    ])

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
                    enterRecoveringState()
                    const snapshot = await recoverVoiceSession(voiceSessionId, lastSeenEventIdRef.current)
                    replayTtsCutoffEventIdRef.current = Number.isFinite(snapshot?.latestEventId)
                        ? Math.max(0, Number(snapshot.latestEventId))
                        : 0
                    if (snapshot?.stopped) {
                        reconnectingRef.current = false
                        stopSession({
                            forced: false,
                            notifyServer: false,
                            statusMessage: snapshot?.stopReason
                                ? `세션이 종료되었습니다. (${snapshot.stopReason})`
                                : '세션이 서버에서 종료되었습니다.'
                        }).catch(() => {
                        })
                        return
                    }

                    applyRecoverySnapshot(snapshot)
                    const recoveryResult = await resumeBufferedAudioAfterRecovery(snapshot)
                    const replayFromEventId = Number.isFinite(snapshot?.replayFromEventId)
                        ? Math.max(0, Number(snapshot.replayFromEventId))
                        : lastSeenEventIdRef.current
                    const eventsUrl = await getVoiceEventsUrl(voiceSessionId, replayFromEventId)
                    closeEventSource()
                    const eventSource = new EventSource(eventsUrl)
                    eventSourceRef.current = eventSource
                    bindSessionEvents(eventSource)
                    reconnectingRef.current = false
                    reconnectAttemptsRef.current = 0
                    setVoiceConnected(true)
                    if (recoveryResult?.droppedDueToMismatch) {
                        setStatus('음성 이벤트 연결은 복구되었지만 질문 컨텍스트가 달라 미전송 음성을 폐기했습니다. 현재 질문에 다시 답변해 주세요.')
                    } else if (snapshot?.gapDetected) {
                        setStatus('이벤트 일부가 누락되어 서버 상태 기준으로 동기화한 뒤 연결을 복구했습니다.')
                    } else {
                        setStatus('음성 이벤트 연결이 복구되었습니다.')
                    }
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
    }, [
        applyRecoverySnapshot,
        bindSessionEvents,
        closeEventSource,
        enterRecoveringState,
        resumeBufferedAudioAfterRecovery,
        setStatus,
        stopSession
    ])

    const handleConnectionError = useCallback(() => {
        if (localStopRef.current || serverStopRef.current) {
            localStopRef.current = false
            serverStopRef.current = false
            return
        }
        if (!sessionActiveRef.current || !voiceSessionIdRef.current) {
            return
        }
        // When SSE is disconnected, defer in-flight audio retry to recovery path
        // to avoid racing retry/reconnect timers and duplicate uploads.
        clearChunkRetryTimer()
        abortInFlightChunkUpload()
        closeEventSource()
        scheduleReconnect()
    }, [abortInFlightChunkUpload, clearChunkRetryTimer, closeEventSource, scheduleReconnect])

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
            clearInFlightTurn()
            chunkSequenceRef.current = 0
            lastSeenEventIdRef.current = 0
            replayTtsCutoffEventIdRef.current = 0
            answerWindowIdRef.current = 0
            captureWindowIdRef.current = NO_WINDOW_ID
            currentQuestionIdRef.current = ''
            captureQuestionIdRef.current = ''
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
                    if (currentState !== SPEECH_STATE.RECOVERING) {
                        resetPendingAudio()
                    }
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
                        captureQuestionIdRef.current = currentQuestionIdRef.current || ''
                        updateSpeechState(SPEECH_STATE.CAPTURING_ANSWER)
                    }
                    speechActiveRef.current = true
                    silenceDurationMsRef.current = 0
                    pendingAudioChunksRef.current.push(bytes)
                    pendingAudioSizeRef.current += bytes.length
                    turnDurationMsRef.current += frameDurationMs
                    if (turnDurationMsRef.current > MAX_TURN_DURATION_MS) {
                        resetPendingAudio()
                        const stop = stopSessionRef.current
                        if (!stop) {
                            setStatus('답변 길이가 너무 길어 세션을 종료했습니다. (최대 2분 30초)')
                            return
                        }
                        stop({
                            forced: true,
                            notifyServer: true,
                            reason: 'turn_duration_exceeded',
                            statusMessage: '답변 길이가 너무 길어 세션을 종료했습니다. (최대 2분 30초)'
                        }).catch(() => {
                        })
                        return
                    }
                    return
                }

                if (!speechActiveRef.current) {
                    return
                }

                silenceDurationMsRef.current += frameDurationMs
                pendingAudioChunksRef.current.push(bytes)
                pendingAudioSizeRef.current += bytes.length
                turnDurationMsRef.current += frameDurationMs
                if (turnDurationMsRef.current > MAX_TURN_DURATION_MS) {
                    resetPendingAudio()
                    const stop = stopSessionRef.current
                    if (!stop) {
                        setStatus('답변 길이가 너무 길어 세션을 종료했습니다. (최대 2분 30초)')
                        return
                    }
                    stop({
                        forced: true,
                        notifyServer: true,
                        reason: 'turn_duration_exceeded',
                        statusMessage: '답변 길이가 너무 길어 세션을 종료했습니다. (최대 2분 30초)'
                    }).catch(() => {
                    })
                    return
                }
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
            currentQuestionIdRef.current = ''
            lastSeenEventIdRef.current = 0
            replayTtsCutoffEventIdRef.current = 0
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
        clearInFlightTurn,
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
