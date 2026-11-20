import {useCallback, useEffect, useRef, useState} from 'react'
import {
    getVoiceEventsUrl,
    openVoiceSession,
    recoverVoiceSession,
    sendVoiceAnswer,
    sendVoiceAudioChunk,
    stopVoiceSession
} from '../../../shared/api/http.js'
import {float32ToInt16, pcm16ToWav, toBase64} from '../../../shared/utils/audioCodec.js'
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
import {loadLocalStt, transcribePcm16} from '../local/localStt.js'
import {
    getLocalTtsStatus,
    loadLocalKoTts,
    loadLocalTts,
    neededLocalTtsModels,
    watchLocalSpeechModels
} from '../local/localSpeech.js'
import {speechModelNote} from '../utils/speechModelNote.js'
import {LOCAL_STT_MODEL, LOCAL_TTS_MODEL} from '../../../shared/constants/models.js'

const VOICE_CHUNK_UPLOAD_TIMEOUT_MS = 3000
const REPLAY_STATE = {
    IDLE: 'IDLE',
    PLAYING: 'PLAYING'
}
// Where the answer is transcribed in the running session.
const STT_MODE = {
    SERVER: 'server', // audio is uploaded and the server calls the STT API
    LOCAL: 'local', // Whisper runs on this device and only the text is sent
    NONE: 'none' // AI off: nothing is transcribed
}

export function useVoiceLifecycle({
                                    sessionId,
                                    feedbackModel,
                                    feedbackProvider = '',
                                    aiEnabled = true,
                                    autoReplay = false,
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
    const [replayState, setReplayState] = useState(REPLAY_STATE.IDLE)
    const [replayAvailable, setReplayAvailable] = useState(false)
    // What the running session was opened with. The toggle in the UI only applies to the next session.
    const [sessionAiEnabled, setSessionAiEnabled] = useState(true)
    // True from the moment an answer is sent until the next question starts playing.
    const [awaitingReview, setAwaitingReview] = useState(false)

    const statusRef = useRef(onStatus)
    const feedbackRef = useRef(onFeedback)
    const refreshWrongNotesRef = useRef(refreshWrongNotes)
    const questionPromptRef = useRef(onQuestionPrompt)

    const settingsRef = useRef({
        feedbackModel,
        feedbackProvider,
        aiEnabled,
        autoReplay,
        voiceSttModel,
        ttsModel,
        feedbackLang,
        voice
    })
    const sessionAiEnabledRef = useRef(true)

    const voiceSessionIdRef = useRef('')
    const eventSourceRef = useRef(null)

    const sessionActiveRef = useRef(false)
    // True while startSession runs (the models can take minutes to download), so a second press of
    // Start does not open a second session.
    const sessionStartingRef = useRef(false)
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
    const currentQuestionTurnIdRef = useRef(0)
    const captureTurnIdRef = useRef(0)
    const captureBlockRef = useRef(false)
    const replayStateRef = useRef(REPLAY_STATE.IDLE)
    const replayCandidateRef = useRef(null)
    const replayConfirmedRef = useRef(null)
    const replayAudioRef = useRef(null)
    const replayAudioUrlRef = useRef('')
    const deferredTtsQueueRef = useRef([])
    const sessionSttModeRef = useRef(STT_MODE.SERVER)
    const transcribingRef = useRef(false)
    const speechUnavailableNoticeShownRef = useRef(false)
    const koreanUnavailableNoticeShownRef = useRef(false)
    // Whether a Korean sentence may fetch the Korean model: only in a session with Korean feedback.
    const koreanModelAllowedRef = useRef(false)
    // The state of the two on-device voices, and which of them the current session reads with.
    const [speechModels, setSpeechModels] = useState({english: {status: 'idle'}, korean: {status: 'idle'}})
    const [sessionVoices, setSessionVoices] = useState({localSpeech: false, koreanWanted: false})
    // Counts session starts; Stop during a start moves it on, which abandons that start.
    const startGenerationRef = useRef(0)
    const onConnectionErrorRef = useRef(() => {
    })

    useEffect(() => {
        statusRef.current = onStatus
    }, [onStatus])

    useEffect(() => watchLocalSpeechModels(setSpeechModels), [])

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
            feedbackProvider,
            aiEnabled,
            autoReplay,
            voiceSttModel,
            ttsModel,
            feedbackLang,
            voice
        }
    }, [feedbackModel, feedbackProvider, aiEnabled, autoReplay, voiceSttModel, ttsModel, feedbackLang, voice])

    const setStatus = useCallback((message) => {
        if (statusRef.current) {
            statusRef.current(message)
        }
    }, [])

    const updateSpeechState = useCallback((nextState) => {
        speechStateRef.current = nextState
        setSpeechState(nextState)
        if (nextState === SPEECH_STATE.PLAYING_QUESTION_TTS
            || nextState === SPEECH_STATE.READY_FOR_ANSWER
            || nextState === SPEECH_STATE.IDLE) {
            setAwaitingReview(false)
        }
    }, [])

    const updateReplayState = useCallback((nextState) => {
        replayStateRef.current = nextState
        setReplayState(nextState)
    }, [])

    const updateReplayConfirmed = useCallback((nextConfirmed) => {
        replayConfirmedRef.current = nextConfirmed
        setReplayAvailable(Boolean(nextConfirmed))
    }, [])

    const parseTurnId = useCallback((value) => {
        const parsed = Number(value)
        if (!Number.isFinite(parsed)) {
            return null
        }
        const turnId = Math.trunc(parsed)
        return turnId > 0 ? turnId : null
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
        const eventId = parseEventId(data)
        const cutoff = replayTtsCutoffEventIdRef.current
        if (cutoff > 0 && eventId !== null && eventId <= cutoff) {
            return true
        }
        if (replayStateRef.current === REPLAY_STATE.PLAYING) {
            deferredTtsQueueRef.current.push(data)
            return true
        }
        return false
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
        captureTurnIdRef.current = 0
    }, [])

    const canCaptureInState = useCallback((state) => {
        if (captureBlockRef.current) {
            return false
        }
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

    const handleLocalSpeechUnavailable = useCallback(() => {
        if (speechUnavailableNoticeShownRef.current) {
            return
        }
        speechUnavailableNoticeShownRef.current = true
        setStatus('이 기기에서 음성을 합성하지 못해 화면으로만 보여 드립니다.')
    }, [setStatus])

    const handleLocalSpeechNotice = useCallback((notice) => {
        if (notice === 'korean-unavailable') {
            // A Korean-feedback session keeps a note about the Korean model in the panel instead.
            if (!koreanModelAllowedRef.current && !koreanUnavailableNoticeShownRef.current) {
                koreanUnavailableNoticeShownRef.current = true
                setStatus('이 세션에서는 한국어 문장을 소리 없이 화면으로만 보여 드립니다.')
            }
        } else if (notice === 'preparing') {
            setStatus('음성을 준비하고 있습니다. 기기에서 합성하므로 잠시 걸릴 수 있습니다.')
        } else if (notice === 'ready') {
            setStatus('음성을 재생합니다.')
        }
    }, [setStatus])

    const {
        clearTtsPlayback,
        enqueueTtsAudio
    } = useTtsPlaybackQueue({
        resetPendingAudio,
        updateSpeechState,
        onQuestionPlaybackCompleted,
        onTtsFailure: handleTtsFailure,
        onLocalSpeechUnavailable: handleLocalSpeechUnavailable,
        onLocalSpeechNotice: handleLocalSpeechNotice,
        koreanModelAllowedRef,
        speechState: SPEECH_STATE
    })

    const clearReplayCandidateForTurn = useCallback((turnId) => {
        const candidate = replayCandidateRef.current
        if (!candidate || candidate.turnId !== turnId) {
            return
        }
        replayCandidateRef.current = null
    }, [])

    const clearReplayCandidateForQuestion = useCallback((questionId) => {
        if (!questionId) {
            return
        }
        const candidate = replayCandidateRef.current
        if (!candidate || candidate.questionId !== questionId) {
            return
        }
        replayCandidateRef.current = null
    }, [])

    const flushDeferredTtsQueue = useCallback(() => {
        const queued = deferredTtsQueueRef.current
        if (!queued.length) {
            return
        }
        deferredTtsQueueRef.current = []
        queued.forEach((item) => {
            enqueueTtsAudio(item)
        })
    }, [enqueueTtsAudio])

    const stopReplayPlayback = useCallback(({
                                                flushDeferred = true,
                                                clearDeferred = false,
                                                statusMessage = ''
                                            } = {}) => {
        const activeAudio = replayAudioRef.current
        if (activeAudio) {
            activeAudio.onended = null
            activeAudio.onerror = null
            activeAudio.pause()
            activeAudio.src = ''
            replayAudioRef.current = null
        }
        if (replayAudioUrlRef.current) {
            URL.revokeObjectURL(replayAudioUrlRef.current)
            replayAudioUrlRef.current = ''
        }
        captureBlockRef.current = false
        updateReplayState(REPLAY_STATE.IDLE)

        if (clearDeferred) {
            deferredTtsQueueRef.current = []
        } else if (flushDeferred) {
            flushDeferredTtsQueue()
        }

        if (statusMessage) {
            setStatus(statusMessage)
        }
    }, [flushDeferredTtsQueue, setStatus, updateReplayState])

    const startReplayPlayback = useCallback(() => {
        const confirmed = replayConfirmedRef.current
        if (!confirmed) {
            return false
        }

        const state = speechStateRef.current
        if (sessionActiveRef.current) {
            const allowedWhileActive = state === SPEECH_STATE.WAITING_TTS || state === SPEECH_STATE.READY_FOR_ANSWER
            if (!allowedWhileActive || state === SPEECH_STATE.CAPTURING_ANSWER) {
                return false
            }
        }

        const sampleRate = Number.isFinite(confirmed.sampleRate) ? confirmed.sampleRate : DEFAULT_SAMPLE_RATE
        const wav = pcm16ToWav(confirmed.pcm16Bytes, sampleRate, 1)
        if (!wav.length) {
            setStatus('다시듣기 재생에 실패했습니다.')
            return false
        }

        const blob = new Blob([wav], {type: 'audio/wav'})
        const url = URL.createObjectURL(blob)
        const audio = new Audio(url)
        replayAudioRef.current = audio
        replayAudioUrlRef.current = url
        if (sessionActiveRef.current && state === SPEECH_STATE.READY_FOR_ANSWER) {
            captureBlockRef.current = true
        }
        updateReplayState(REPLAY_STATE.PLAYING)

        const finish = ({failed = false} = {}) => {
            stopReplayPlayback({
                flushDeferred: true,
                statusMessage: failed ? '다시듣기 재생에 실패했습니다.' : ''
            })
        }
        audio.onended = () => finish()
        audio.onerror = () => finish({failed: true})
        audio.play().catch(() => {
            finish({failed: true})
        })
        return true
    }, [setStatus, stopReplayPlayback, updateReplayState])

    const getReplayDisabledReason = useCallback(() => {
        if (replayState === REPLAY_STATE.PLAYING) {
            return ''
        }
        if (!replayAvailable) {
            return '다시 들을 수 있는 응답이 없습니다.'
        }
        if (!sessionActive) {
            return ''
        }
        if (speechState === SPEECH_STATE.CAPTURING_ANSWER) {
            return '답변을 수집 중에는 다시듣기를 사용할 수 없습니다.'
        }
        const allowed = speechState === SPEECH_STATE.WAITING_TTS || speechState === SPEECH_STATE.READY_FOR_ANSWER
        if (!allowed) {
            return '지금은 다시듣기를 사용할 수 없습니다.'
        }
        return ''
    }, [replayAvailable, replayState, sessionActive, speechState])

    const handleReplayAction = useCallback(() => {
        if (replayStateRef.current === REPLAY_STATE.PLAYING) {
            stopReplayPlayback({flushDeferred: true})
            return
        }
        if (getReplayDisabledReason()) {
            return
        }
        startReplayPlayback()
    }, [getReplayDisabledReason, startReplayPlayback, stopReplayPlayback])

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

    const stopCapture = useCallback((statusMessage = '', {preserveReplay = false} = {}) => {
        resetPendingAudio()
        clearInFlightTurn()
        clearTtsPlayback()
        if (preserveReplay && replayStateRef.current === REPLAY_STATE.PLAYING) {
            // The session ended on its own while the learner's answer is still playing back
            // (last question with AI feedback off). Let it finish; only drop what was queued behind it.
            deferredTtsQueueRef.current = []
            captureBlockRef.current = false
        } else {
            stopReplayPlayback({flushDeferred: false, clearDeferred: true})
        }

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
    }, [
        clearInFlightTurn,
        clearTtsPlayback,
        refreshAudioDeviceStatus,
        resetPendingAudio,
        setStatus,
        stopReplayPlayback
    ])

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
        const turnId = parseTurnId(snapshot?.currentTurnId) || 0
        currentQuestionIdRef.current = questionNode?.id || ''
        currentQuestionTurnIdRef.current = questionNode ? turnId : 0
        if (questionNode) {
            questionPromptRef.current?.({
                turnId,
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
    }, [parseTurnId, updateSpeechState])

    const handleSttFinal = useCallback((data) => {
        const turnId = parseTurnId(data?.turnId)
        if (turnId === null) {
            return
        }
        const candidate = replayCandidateRef.current
        if (!candidate || candidate.turnId !== turnId) {
            return
        }
        updateReplayConfirmed({
            turnId: candidate.turnId,
            questionId: candidate.questionId,
            sampleRate: candidate.sampleRate,
            pcm16Bytes: candidate.pcm16Bytes,
            confirmedAt: Date.now(),
            confirmedBy: 'stt.final'
        })
        replayCandidateRef.current = null

        // AI off: there is no feedback to read back, so the learner hears their own answer instead.
        // The next question is read aloud after this playback (it waits in deferredTtsQueue).
        const shouldAutoReplay = sessionActiveRef.current
            && !sessionAiEnabledRef.current
            && settingsRef.current.autoReplay
        if (shouldAutoReplay && startReplayPlayback()) {
            setStatus('내 응답을 다시 듣는 중입니다. 재생이 끝나면 다음 질문으로 넘어갑니다.')
        }
    }, [parseTurnId, setStatus, startReplayPlayback, updateReplayConfirmed])

    const handleSttSkipped = useCallback((data) => {
        const skippedTurnId = parseTurnId(data?.turnId)
        if (skippedTurnId !== null) {
            clearReplayCandidateForTurn(skippedTurnId)
        }
        if (!sessionActiveRef.current || !voiceSessionIdRef.current || localStopRef.current || serverStopRef.current) {
            return
        }
        resetPendingAudio()
        updateSpeechState(SPEECH_STATE.READY_FOR_ANSWER)
    }, [clearReplayCandidateForTurn, parseTurnId, resetPendingAudio, updateSpeechState])

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
            if (turn.kind === 'answer') {
                const answer = turn.text ? {sequence: turn.sequence, text: turn.text} : {sequence: turn.sequence}
                await sendVoiceAnswer(turn.voiceSessionId, answer, {
                    signal: abortController.signal
                })
            } else {
                await sendVoiceAudioChunk(turn.voiceSessionId, {
                    pcm16Base64: turn.pcm16Base64,
                    sampleRate: turn.sampleRate,
                    sequence: turn.sequence
                }, {
                    signal: abortController.signal
                })
            }
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
            clearReplayCandidateForQuestion(localQuestionId)
            const captureTurnId = parseTurnId(captureTurnIdRef.current)
            if (captureTurnId !== null) {
                clearReplayCandidateForTurn(captureTurnId)
            }
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
        if (inFlightTurnRef.current || transcribingRef.current) {
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
        const capturedTurnId = parseTurnId(captureTurnIdRef.current) || parseTurnId(currentQuestionTurnIdRef.current)

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
        if (capturedTurnId !== null) {
            replayCandidateRef.current = {
                turnId: capturedTurnId,
                questionId: capturedQuestionId,
                sampleRate,
                pcm16Bytes: merged,
                capturedAt: Date.now()
            }
        } else {
            replayCandidateRef.current = null
        }

        pendingAudioChunksRef.current = []
        pendingAudioSizeRef.current = 0
        speechActiveRef.current = false
        silenceDurationMsRef.current = 0
        turnDurationMsRef.current = 0
        captureQuestionIdRef.current = ''
        captureTurnIdRef.current = 0

        updateSpeechState(SPEECH_STATE.WAITING_TTS)
        setAwaitingReview(true)
        // Server STT uploads the recording. Local STT transcribes it here and sends only the text.
        // AI off keeps it on the device (it is only used for replay) and reports the end of the answer.
        const sttMode = sessionSttModeRef.current
        let answerText = ''
        if (sttMode === STT_MODE.LOCAL) {
            let failure = ''
            transcribingRef.current = true
            try {
                answerText = (await transcribePcm16(merged, sampleRate)).trim()
            } catch (error) {
                failure = error?.message || '알 수 없는 오류'
            } finally {
                transcribingRef.current = false
            }
            if (!sessionActiveRef.current || voiceSessionIdRef.current !== voiceSessionId) {
                return
            }
            if (!answerText) {
                // Same outcome as the server reporting stt.skipped: answer the question again.
                if (capturedTurnId !== null) {
                    clearReplayCandidateForTurn(capturedTurnId)
                }
                setStatus(failure
                    ? `로컬 음성 인식에 실패했습니다. 다시 답변해 주세요. (${failure})`
                    : '음성이 인식되지 않아 다시 답변해 주세요.')
                updateSpeechState(SPEECH_STATE.READY_FOR_ANSWER)
                return
            }
        }
        const uploadsAudio = sttMode === STT_MODE.SERVER
        inFlightTurnRef.current = {
            kind: uploadsAudio ? 'audio' : 'answer',
            voiceSessionId,
            pcm16Base64: uploadsAudio ? toBase64(merged) : '',
            text: answerText,
            sampleRate,
            sequence: nextSequence,
            capturedWindowId,
            questionId: capturedQuestionId,
            turnId: capturedTurnId || 0,
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
    }, [
        canCaptureInState,
        clearReplayCandidateForQuestion,
        clearReplayCandidateForTurn,
        parseTurnId,
        scheduleChunkRetry,
        setStatus,
        resetPendingAudio,
        updateSpeechState,
        uploadInFlightTurn
    ])

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
                                               statusMessage = '세션을 종료했습니다.',
                                               preserveReplay = false
                                           } = {}) => {
        if (sessionStartingRef.current && !sessionActiveRef.current) {
            // Stop while the session starts (models may take minutes to download): the start is
            // abandoned. Loads already running go on in the background and stay usable.
            startGenerationRef.current += 1
            sessionStartingRef.current = false
            const startingVoiceSessionId = voiceSessionIdRef.current
            voiceSessionIdRef.current = ''
            closeEventSource()
            if (notifyServer && startingVoiceSessionId) {
                stopVoiceSession(startingVoiceSessionId, {forced, reason}).catch(() => {
                })
            }
            setVoiceConnected(false)
            setSessionPhase('IDLE')
            updateSpeechState(SPEECH_STATE.IDLE)
            if (statusMessage) {
                setStatus('세션 시작을 취소했습니다.')
            }
            return
        }
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
        stopCapture('', {preserveReplay})

        const voiceSessionId = voiceSessionIdRef.current
        if (notifyServer && voiceSessionId) {
            await stopVoiceSession(voiceSessionId, {forced, reason}).catch(() => {
            })
        }

        closeEventSource()
        voiceSessionIdRef.current = ''
        currentQuestionIdRef.current = ''
        currentQuestionTurnIdRef.current = 0
        replayCandidateRef.current = null
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
                currentQuestionTurnIdRef.current = parseTurnId(question?.turnId) || 0
                questionPromptRef.current?.(question)
            },
            getQuestionPromptStatus: () => (
                replayStateRef.current === REPLAY_STATE.PLAYING
                    ? '내 응답을 다시 듣는 중입니다. 재생이 끝나면 다음 질문을 읽어 드립니다.'
                    : ''
            ),
            onFeedback: (feedback) => {
                feedbackRef.current?.(feedback)
            },
            refreshWrongNotes: refreshWrongNotesAction,
            stopSession,
            shouldResumeImmediatelyOnQuestionPrompt,
            enqueueTtsAudio,
            handleTtsFailure,
            onSttFinal: handleSttFinal,
            onAnswerCaptured: handleSttFinal,
            onSttSkipped: handleSttSkipped,
            shouldProcessEvent,
            shouldSkipReplayTts,
            onConnectionError: () => {
                onConnectionErrorRef.current?.()
            }
        })
    }, [
        enqueueTtsAudio,
        handleSttFinal,
        handleSttSkipped,
        handleTtsFailure,
        parseTurnId,
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

    // The model is downloaded once (first use) and then served from the browser cache. report is
    // the status setter of the start that asked, so an abandoned start shows nothing more.
    const prepareLocalStt = useCallback(async (report) => {
        report('기기에서 실행할 음성 인식 모델을 준비하고 있습니다.')
        let lastReported = -1
        try {
            await loadLocalStt(({progress}) => {
                const percent = Math.floor(progress / 10) * 10
                if (percent > lastReported) {
                    lastReported = percent
                    report(`음성 인식 모델을 내려받는 중입니다. (${percent}%)`)
                }
            })
        } catch (error) {
            throw new Error(
                `로컬 음성 인식 모델을 불러오지 못했습니다. 네트워크를 확인하거나 설정에서 STT 모델을 바꿔 주세요. (${error?.message || '알 수 없는 오류'})`
            )
        }
    }, [])

    // Downloads (first use) and loads one on-device speech model, reporting which one it is. On first
    // use the backend fetches the files before the percentage can move, so the size is said up front.
    // A model that is already loaded is not mentioned.
    const loadSpeechModel = useCallback(async (load, getStatus, label, size, report) => {
        if (getStatus() === 'ready') {
            await load()
            return
        }
        report(`기기에서 실행할 ${label} 모델을 준비하고 있습니다. 처음 사용할 때는 ${size}를 내려받으므로 몇 분 걸릴 수 있습니다.`)
        let lastReported = -1
        await load(({progress}) => {
            const percent = Math.floor(progress / 10) * 10
            if (percent > lastReported) {
                lastReported = percent
                report(`${label} 모델을 내려받는 중입니다. (${percent}%)`)
            }
        })
    }, [])

    // English (questions) uses Kokoro and is loaded before the session opens: the first question
    // needs it. The Korean model (Korean feedback only, 263 MB) loads in the background and does
    // not hold up the start; Korean sentences before it is ready are shown without sound.
    const prepareLocalTts = useCallback(async ({feedbackLanguage, report}) => {
        const needed = neededLocalTtsModels(feedbackLanguage)
        if (needed.korean) {
            loadLocalKoTts().catch(() => {
            })
        }
        if (needed.english) {
            try {
                await loadSpeechModel(loadLocalTts, getLocalTtsStatus, '영어 음성 합성', '약 90MB', report)
            } catch (_error) {
                // The session opens all the same (AI on or off): questions are shown on screen, and
                // the next English sentence retries the load in the background.
                report('음성 합성 모델을 불러오지 못해 질문을 화면으로만 보여 드립니다.')
            }
        }
    }, [loadSpeechModel])

    const startSession = useCallback(async () => {
        if (sessionActiveRef.current || sessionStartingRef.current) {
            return
        }

        if (!navigator.mediaDevices || !navigator.mediaDevices.getUserMedia) {
            setAudioPermission('unsupported')
            setStatus('이 환경에서는 오디오 입력 장치를 지원하지 않습니다.')
            return
        }

        sessionStartingRef.current = true
        const generation = startGenerationRef.current + 1
        startGenerationRef.current = generation
        const abandoned = () => startGenerationRef.current !== generation
        const report = (message) => {
            if (!abandoned()) {
                setStatus(message)
            }
        }
        try {
            setSessionPhase('STARTING')
            localStopRef.current = false
            serverStopRef.current = false
            stopReplayPlayback({flushDeferred: false, clearDeferred: true})
            replayCandidateRef.current = null
            updateReplayConfirmed(null)
            resetReconnectState()
            clearInFlightTurn()
            chunkSequenceRef.current = 0
            lastSeenEventIdRef.current = 0
            replayTtsCutoffEventIdRef.current = 0
            answerWindowIdRef.current = 0
            captureWindowIdRef.current = NO_WINDOW_ID
            currentQuestionIdRef.current = ''
            currentQuestionTurnIdRef.current = 0
            captureQuestionIdRef.current = ''
            captureTurnIdRef.current = 0
            updateSpeechState(SPEECH_STATE.WAITING_TTS)

            const aiOn = settingsRef.current.aiEnabled !== false
            sessionAiEnabledRef.current = aiOn
            setSessionAiEnabled(aiOn)
            setAwaitingReview(false)
            speechUnavailableNoticeShownRef.current = false
            koreanUnavailableNoticeShownRef.current = false
            const feedbackLanguage = aiOn ? settingsRef.current.feedbackLang : null
            koreanModelAllowedRef.current = feedbackLanguage === 'ko'
            // Speech is read on this device when AI is off or the free local TTS model is selected.
            const localSpeech = !aiOn || settingsRef.current.ttsModel === LOCAL_TTS_MODEL
            setSessionVoices({localSpeech, koreanWanted: localSpeech && feedbackLanguage === 'ko'})
            transcribingRef.current = false
            sessionSttModeRef.current = !aiOn
                ? STT_MODE.NONE
                : (settingsRef.current.voiceSttModel === LOCAL_STT_MODEL ? STT_MODE.LOCAL : STT_MODE.SERVER)
            if (sessionSttModeRef.current === STT_MODE.LOCAL) {
                await prepareLocalStt(report)
                if (abandoned()) {
                    return
                }
            }
            if (localSpeech) {
                await prepareLocalTts({feedbackLanguage, report})
                if (abandoned()) {
                    return
                }
            }

            const created = await openVoiceSession({
                sessionId,
                aiEnabled: aiOn,
                feedbackProvider: settingsRef.current.feedbackProvider || undefined,
                feedbackModel: settingsRef.current.feedbackModel,
                feedbackLanguage: settingsRef.current.feedbackLang,
                sttModel: settingsRef.current.voiceSttModel,
                ttsModel: settingsRef.current.ttsModel,
                ttsVoice: settingsRef.current.voice
            })
            const voiceSessionId = created?.voiceSessionId
            if (abandoned()) {
                if (voiceSessionId) {
                    stopVoiceSession(voiceSessionId, {forced: true, reason: 'user_stop'}).catch(() => {
                    })
                }
                return
            }
            if (!voiceSessionId) {
                throw new Error('음성 세션 ID를 받지 못했습니다.')
            }
            voiceSessionIdRef.current = voiceSessionId

            const eventsUrl = await getVoiceEventsUrl(voiceSessionId)
            if (abandoned()) {
                // Stop already closed the voice session it found.
                return
            }
            const eventSource = new EventSource(eventsUrl)
            eventSourceRef.current = eventSource
            bindSessionEvents(eventSource)

            const stream = await navigator.mediaDevices.getUserMedia({audio: true})
            if (abandoned()) {
                stream.getTracks().forEach((track) => track.stop())
                return
            }
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
                        captureTurnIdRef.current = parseTurnId(currentQuestionTurnIdRef.current) || 0
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
            setStatus(aiOn
                ? '세션을 시작했습니다. 질문을 듣고 답변해 주세요.'
                : '세션을 시작했습니다. AI 없이 진행합니다. 질문을 듣고 답변해 주세요.')
            refreshAudioDeviceStatus().catch(() => {
            })
        } catch (error) {
            if (abandoned()) {
                // Stop has already put everything back; what is running now belongs to a newer start.
                return
            }
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
            currentQuestionTurnIdRef.current = 0
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
        } finally {
            if (!abandoned()) {
                sessionStartingRef.current = false
            }
        }
    }, [
        bindSessionEvents,
        canCaptureInState,
        clearInFlightTurn,
        closeEventSource,
        flushPendingAudio,
        parseTurnId,
        prepareLocalStt,
        prepareLocalTts,
        refreshAudioDeviceStatus,
        resetPendingAudio,
        resetReconnectState,
        sessionId,
        setAudioPermission,
        setStatus,
        stopCapture,
        stopReplayPlayback,
        updateReplayConfirmed,
        updateSpeechState
    ])

    const syncVoiceSettings = useCallback(async () => {
        setStatus('설정은 다음 세션 시작 시 반영됩니다.')
    }, [setStatus])

    // The English note is about a reload during the session; before it opens, the start reports
    // its own load in the status bar.
    const voiceNote = sessionPhase === 'IDLE' || sessionPhase === 'STOPPING'
        ? ''
        : speechModelNote({
            english: sessionPhase === 'ACTIVE' ? speechModels.english : {status: 'idle'},
            korean: speechModels.korean,
            ...sessionVoices
        })

    const replayButtonDisabledReason = getReplayDisabledReason()
    const replayButtonDisabled = replayState !== REPLAY_STATE.PLAYING && Boolean(replayButtonDisabledReason)
    const replayButtonLabel = replayState === REPLAY_STATE.PLAYING ? '중지' : '내 응답 재생'

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
        handleAudioQuickAction,
        replayButtonLabel,
        replayButtonDisabled,
        replayButtonDisabledReason,
        handleReplayAction,
        replayPlaying: replayState === REPLAY_STATE.PLAYING,
        sessionAiEnabled,
        awaitingReview,
        speechModelNote: voiceNote
    }
}
