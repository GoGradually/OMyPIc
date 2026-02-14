import {useCallback, useEffect, useRef, useState} from 'react'
import {
    getVoiceEventsUrl,
    openVoiceSession,
    sendVoiceAudioChunk,
    stopVoiceSession
} from '../../../shared/api/http.js'
import {buildFeedbackFromVoice} from '../utils/voiceEvent.js'
import {float32ToInt16, fromBase64, toBase64} from '../../../shared/utils/audioCodec.js'

const INITIAL_AUDIO_DEVICE_STATUS = {
    inputCount: 0,
    outputCount: 0,
    activeInputLabel: '',
    liveInput: false,
    lastCheckedAt: ''
}
const AUDIO_PROCESS_BUFFER_SIZE = 2048
const DEFAULT_SAMPLE_RATE = 16000
const VAD_RMS_THRESHOLD = 0.018
const VAD_SILENCE_MS = 1500
const MIN_TURN_DURATION_MS = 300
const NO_WINDOW_ID = -1
const SPEECH_STATE = {
    IDLE: 'IDLE',
    WAITING_TTS: 'WAITING_TTS',
    PLAYING_FEEDBACK_TTS: 'PLAYING_FEEDBACK_TTS',
    PLAYING_QUESTION_TTS: 'PLAYING_QUESTION_TTS',
    READY_FOR_ANSWER: 'READY_FOR_ANSWER',
    CAPTURING_ANSWER: 'CAPTURING_ANSWER',
    STOPPING: 'STOPPING'
}

function getCurrentTimeLabel() {
    return new Date().toLocaleTimeString('ko-KR', {hour12: false})
}

function parseSseData(raw) {
    if (!raw) {
        return {}
    }
    try {
        return JSON.parse(raw)
    } catch (_error) {
        return {}
    }
}

function joinByteChunks(chunks, totalSize) {
    const merged = new Uint8Array(totalSize)
    let offset = 0
    for (const chunk of chunks) {
        merged.set(chunk, offset)
        offset += chunk.length
    }
    return merged
}

function computeRms(float32Array) {
    if (!float32Array || !float32Array.length) {
        return 0
    }
    let sum = 0
    for (let i = 0; i < float32Array.length; i += 1) {
        const sample = float32Array[i]
        sum += sample * sample
    }
    return Math.sqrt(sum / float32Array.length)
}

export function useVoiceSession({
                                       sessionId,
                                       feedbackModel,
                                       voiceSttModel,
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
    const [audioPermission, setAudioPermission] = useState('unknown')
    const [audioDeviceStatus, setAudioDeviceStatus] = useState(INITIAL_AUDIO_DEVICE_STATUS)

    const statusRef = useRef(onStatus)
    const feedbackRef = useRef(onFeedback)
    const refreshWrongNotesRef = useRef(refreshWrongNotes)
    const questionPromptRef = useRef(onQuestionPrompt)

    const settingsRef = useRef({
        feedbackModel,
        voiceSttModel,
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
    const ttsPlaybackQueueRef = useRef([])
    const ttsPlayingRef = useRef(false)
    const ttsActiveAudioRef = useRef(null)
    const ttsReceiveOrderRef = useRef(0)
    const answerWindowIdRef = useRef(0)
    const captureWindowIdRef = useRef(NO_WINDOW_ID)

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
            feedbackLang,
            voice
        }
    }, [feedbackModel, voiceSttModel, feedbackLang, voice])

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

    const clearTtsPlayback = useCallback(() => {
        ttsPlaybackQueueRef.current = []
        ttsPlayingRef.current = false
        ttsReceiveOrderRef.current = 0
        const activeAudio = ttsActiveAudioRef.current
        if (!activeAudio) {
            return
        }
        activeAudio.onended = null
        activeAudio.onerror = null
        activeAudio.pause()
        activeAudio.src = ''
        ttsActiveAudioRef.current = null
    }, [])

    const refreshAudioDeviceStatus = useCallback(async ({requestPermission = false} = {}) => {
        if (!navigator.mediaDevices || !navigator.mediaDevices.enumerateDevices) {
            setAudioPermission('unsupported')
            setAudioDeviceStatus({
                ...INITIAL_AUDIO_DEVICE_STATUS,
                lastCheckedAt: getCurrentTimeLabel()
            })
            return
        }

        if (requestPermission && !sessionActiveRef.current) {
            try {
                const permissionStream = await navigator.mediaDevices.getUserMedia({audio: true})
                permissionStream.getTracks().forEach((track) => track.stop())
                setAudioPermission('granted')
                setStatus('오디오 장비 권한을 허용했습니다.')
            } catch (error) {
                if (error?.name === 'NotAllowedError' || error?.name === 'SecurityError') {
                    setAudioPermission('denied')
                    setStatus('마이크 권한이 거부되었습니다. 브라우저 설정에서 허용해 주세요.')
                } else if (error?.name === 'NotFoundError') {
                    setStatus('사용 가능한 마이크 장치를 찾지 못했습니다.')
                } else {
                    setStatus(`오디오 권한 요청 실패: ${error?.message || '알 수 없는 오류'}`)
                }
            }
        }

        try {
            const devices = await navigator.mediaDevices.enumerateDevices()
            const audioInputs = devices.filter((device) => device.kind === 'audioinput')
            const audioOutputs = devices.filter((device) => device.kind === 'audiooutput')
            const liveTrack = streamRef.current?.getAudioTracks?.()[0] || null
            const activeInputLabel = liveTrack?.label || audioInputs.find((device) => device.label)?.label || ''

            if (audioInputs.some((device) => Boolean(device.label))) {
                setAudioPermission((prev) => (prev === 'denied' ? prev : 'granted'))
            }

            setAudioDeviceStatus({
                inputCount: audioInputs.length,
                outputCount: audioOutputs.length,
                activeInputLabel,
                liveInput: Boolean(liveTrack && liveTrack.readyState === 'live'),
                lastCheckedAt: getCurrentTimeLabel()
            })
        } catch (error) {
            setStatus(`오디오 장치 상태 확인 실패: ${error?.message || '알 수 없는 오류'}`)
        }
    }, [setStatus])

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
    }, [closeEventSource, flushPendingAudio, resetPendingAudio, setStatus, stopCapture, updateSpeechState])

    const handleTtsFailure = useCallback((message = 'TTS 출력 실패로 세션을 종료했습니다.') => {
        stopSession({
            forced: true,
            notifyServer: true,
            reason: 'tts_failed',
            statusMessage: message
        }).catch(() => {
        })
    }, [stopSession])

    const playNextTtsAudio = useCallback(() => {
        if (ttsPlayingRef.current) {
            return
        }
        const nextItem = ttsPlaybackQueueRef.current.shift()
        if (!nextItem) {
            return
        }

        resetPendingAudio()
        if (nextItem.role === 'feedback') {
            updateSpeechState(SPEECH_STATE.PLAYING_FEEDBACK_TTS)
        } else {
            updateSpeechState(SPEECH_STATE.PLAYING_QUESTION_TTS)
        }

        const bytes = fromBase64(nextItem.audioBase64)
        if (!bytes.length) {
            handleTtsFailure('TTS 출력 실패로 세션을 종료했습니다.')
            return
        }

        const blob = new Blob([bytes], {type: nextItem.mimeType || 'audio/wav'})
        const url = URL.createObjectURL(blob)
        const audio = new Audio(url)
        ttsActiveAudioRef.current = audio
        ttsPlayingRef.current = true

        const finishPlayback = ({failed = false} = {}) => {
            if (ttsActiveAudioRef.current === audio) {
                ttsActiveAudioRef.current = null
            }
            ttsPlayingRef.current = false
            URL.revokeObjectURL(url)
            if (failed) {
                handleTtsFailure('TTS 출력 실패로 세션을 종료했습니다.')
                return
            }
            if (nextItem.role === 'question') {
                answerWindowIdRef.current += 1
                updateSpeechState(SPEECH_STATE.READY_FOR_ANSWER)
            } else {
                updateSpeechState(SPEECH_STATE.WAITING_TTS)
            }
            playNextTtsAudio()
        }

        audio.onended = () => finishPlayback()
        audio.onerror = () => finishPlayback({failed: true})
        audio.play().catch(() => {
            finishPlayback({failed: true})
        })
    }, [handleTtsFailure, resetPendingAudio, updateSpeechState])

    const enqueueTtsAudio = useCallback((data) => {
        const audioBase64 = data?.audio || ''
        if (!audioBase64) {
            return
        }
        const parsedSequence = Number(data?.sequence)
        const sequence = Number.isFinite(parsedSequence) ? parsedSequence : null
        const roleValue = (data?.role || data?.phase || '').toString().toLowerCase()
        const role = roleValue === 'feedback' ? 'feedback' : 'question'
        const item = {
            role,
            sequence,
            order: ++ttsReceiveOrderRef.current,
            audioBase64,
            mimeType: data?.mimeType || 'audio/wav'
        }
        ttsPlaybackQueueRef.current.push(item)
        ttsPlaybackQueueRef.current.sort((left, right) => {
            const leftHasSequence = left.sequence !== null
            const rightHasSequence = right.sequence !== null
            if (leftHasSequence && rightHasSequence) {
                return left.sequence - right.sequence
            }
            if (leftHasSequence) {
                return -1
            }
            if (rightHasSequence) {
                return 1
            }
            return left.order - right.order
        })
        playNextTtsAudio()
    }, [playNextTtsAudio])

    const bindVoiceEvents = useCallback((eventSource) => {
        eventSource.addEventListener('session.ready', () => {
            setVoiceConnected(true)
            setStatus('음성 세션에 연결되었습니다.')
        })

        eventSource.addEventListener('stt.final', (event) => {
            const data = parseSseData(event?.data)
            const text = data?.text || ''
            setPartialTranscript('')
            setTranscript(text)
            setUserText(text)
        })

        eventSource.addEventListener('question.prompt', (event) => {
            const data = parseSseData(event?.data)
            const hasStructuredQuestion = data && Object.prototype.hasOwnProperty.call(data, 'question')
            const questionNode = hasStructuredQuestion ? data?.question : data
            const selection = data?.selection || {}
            const normalizedQuestion = {
                questionId: questionNode?.id || questionNode?.questionId || '',
                text: questionNode?.text || '',
                group: questionNode?.group || '',
                exhausted: Boolean(selection?.exhausted),
                selectionReason: selection?.reason || ''
            }
            resetPendingAudio()
            updateSpeechState(SPEECH_STATE.WAITING_TTS)
            if (questionPromptRef.current) {
                questionPromptRef.current(normalizedQuestion)
            }
            if (normalizedQuestion.exhausted) {
                setStatus('모든 질문을 완료했습니다.')
            } else if (normalizedQuestion.text) {
                setStatus('질문이 도착했습니다. 답변을 시작해 주세요.')
            }
        })

        eventSource.addEventListener('feedback.final', (event) => {
            const data = parseSseData(event?.data)
            if (feedbackRef.current) {
                feedbackRef.current(buildFeedbackFromVoice(data))
            }
            if (refreshWrongNotesRef.current) {
                refreshWrongNotesRef.current().catch(() => {
                })
            }
        })

        eventSource.addEventListener('session.stopped', (event) => {
            serverStopRef.current = true
            const data = parseSseData(event?.data)
            const reason = data?.reason || ''
            const endedByExhausted = reason === 'QUESTION_EXHAUSTED' || reason === 'question_exhausted'
            stopSession({
                forced: false,
                notifyServer: false,
                statusMessage: endedByExhausted
                    ? '모든 질문을 완료하여 세션이 자동 종료되었습니다.'
                    : '세션이 서버에서 종료되었습니다.'
            }).catch(() => {
            })
        })

        eventSource.addEventListener('tts.audio', (event) => {
            const data = parseSseData(event?.data)
            enqueueTtsAudio(data)
        })

        eventSource.addEventListener('tts.error', (event) => {
            const data = parseSseData(event?.data)
            const message = data?.message
                ? `TTS 출력 실패로 세션을 종료했습니다. (${data.message})`
                : 'TTS 출력 실패로 세션을 종료했습니다.'
            handleTtsFailure(message)
        })

        eventSource.addEventListener('error', (event) => {
            const data = parseSseData(event?.data)
            if (data?.message) {
                setStatus(`오류: ${data.message}`)
            }
        })

        eventSource.onerror = () => {
            if (localStopRef.current || serverStopRef.current) {
                localStopRef.current = false
                serverStopRef.current = false
                return
            }
            stopSession({
                forced: false,
                notifyServer: false,
                statusMessage: '음성 이벤트 연결이 종료되어 세션을 중단했습니다.'
            }).catch(() => {
            })
        }
    }, [enqueueTtsAudio, handleTtsFailure, resetPendingAudio, setStatus, stopSession, updateSpeechState])

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
            answerWindowIdRef.current = 0
            captureWindowIdRef.current = NO_WINDOW_ID
            updateSpeechState(SPEECH_STATE.WAITING_TTS)

            const created = await openVoiceSession({
                sessionId,
                feedbackModel: settingsRef.current.feedbackModel,
                feedbackLanguage: settingsRef.current.feedbackLang,
                sttModel: settingsRef.current.voiceSttModel,
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
            bindVoiceEvents(eventSource)

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
    }, [bindVoiceEvents, canCaptureInState, closeEventSource, flushPendingAudio, refreshAudioDeviceStatus, resetPendingAudio, sessionId, setStatus, stopCapture, updateSpeechState])

    const handleAudioQuickAction = useCallback(async () => {
        if (audioPermission === 'unsupported') {
            return
        }
        if (audioPermission === 'granted') {
            await refreshAudioDeviceStatus()
            setStatus('오디오 연결 상태를 확인했습니다.')
            return
        }
        await refreshAudioDeviceStatus({requestPermission: true})
    }, [audioPermission, refreshAudioDeviceStatus, setStatus])

    const syncVoiceSettings = useCallback(async () => {
        setStatus('설정은 다음 세션 시작 시 반영됩니다.')
    }, [setStatus])

    useEffect(() => {
        return () => {
            stopSession({forced: false, notifyServer: false, statusMessage: ''}).catch(() => {
            })
        }
    }, [stopSession])

    useEffect(() => {
        refreshAudioDeviceStatus().catch(() => {
        })
    }, [refreshAudioDeviceStatus])

    useEffect(() => {
        if (!navigator.mediaDevices || !navigator.mediaDevices.addEventListener) {
            return
        }

        const handleDeviceChange = () => {
            refreshAudioDeviceStatus().catch(() => {
            })
        }

        navigator.mediaDevices.addEventListener('devicechange', handleDeviceChange)
        return () => {
            navigator.mediaDevices.removeEventListener('devicechange', handleDeviceChange)
        }
    }, [refreshAudioDeviceStatus])

    useEffect(() => {
        if (!navigator.permissions || !navigator.permissions.query) {
            return
        }

        let cancelled = false
        let permissionStatus = null

        async function observeMicrophonePermission() {
            try {
                permissionStatus = await navigator.permissions.query({name: 'microphone'})
                if (cancelled || !permissionStatus) {
                    return
                }
                setAudioPermission(permissionStatus.state)
                permissionStatus.onchange = () => {
                    setAudioPermission(permissionStatus.state)
                    refreshAudioDeviceStatus().catch(() => {
                    })
                }
            } catch (_error) {
            }
        }

        observeMicrophonePermission().catch(() => {
        })

        return () => {
            cancelled = true
            if (permissionStatus) {
                permissionStatus.onchange = null
            }
        }
    }, [refreshAudioDeviceStatus])

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
