import {useCallback, useEffect, useRef, useState} from 'react'
import {closeRealtime, connectRealtime, getApiKey, sendRealtime, subscribeRealtime} from '../../../shared/api/http.js'
import {float32ToInt16, mergeAudioChunks, pcm16ToWav, toBase64} from '../../../shared/utils/audioCodec.js'
import {buildFeedbackFromRealtime, parseRealtimeEnvelope} from '../utils/realtimeEvent.js'

const INITIAL_AUDIO_DEVICE_STATUS = {
    inputCount: 0,
    outputCount: 0,
    activeInputLabel: '',
    liveInput: false,
    lastCheckedAt: ''
}
const AUDIO_PROCESS_BUFFER_SIZE = 2048

function getCurrentTimeLabel() {
    return new Date().toLocaleTimeString('ko-KR', {hour12: false})
}

export function useRealtimeSession({
                                       sessionId,
                                       provider,
                                       feedbackModel,
                                       realtimeConversationModel,
                                       realtimeSttModel,
                                       feedbackLang,
                                       voice,
                                       onStatus,
                                       onFeedback,
                                       refreshWrongNotes,
                                       onQuestionPrompt
                                   }) {
    const [sessionActive, setSessionActive] = useState(false)
    const [sessionPhase, setSessionPhase] = useState('IDLE')
    const [realtimeConnected, setRealtimeConnected] = useState(false)
    const [partialTranscript, setPartialTranscript] = useState('')
    const [transcript, setTranscript] = useState('')
    const [userText, setUserText] = useState('')
    const [audioPermission, setAudioPermission] = useState('unknown')
    const [audioDeviceStatus, setAudioDeviceStatus] = useState(INITIAL_AUDIO_DEVICE_STATUS)

    const statusRef = useRef(onStatus)
    const feedbackRef = useRef(onFeedback)
    const refreshWrongNotesRef = useRef(refreshWrongNotes)
    const questionPromptRef = useRef(onQuestionPrompt)

    const settingsRef = useRef({
        provider,
        feedbackModel,
        realtimeConversationModel,
        realtimeSttModel,
        feedbackLang,
        voice
    })

    const realtimeSocketIdRef = useRef(null)
    const connectedRealtimeModelsRef = useRef({conversationModel: '', sttModel: ''})
    const ttsChunksRef = useRef(new Map())

    const sessionActiveRef = useRef(false)
    const ignoreIncomingRef = useRef(false)
    const localStopRef = useRef(false)
    const serverStopRef = useRef(false)
    const sessionPhaseRef = useRef('IDLE')

    const streamRef = useRef(null)
    const audioContextRef = useRef(null)
    const sourceNodeRef = useRef(null)
    const processorRef = useRef(null)

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
            provider,
            feedbackModel,
            realtimeConversationModel,
            realtimeSttModel,
            feedbackLang,
            voice
        }
    }, [provider, feedbackModel, realtimeConversationModel, realtimeSttModel, feedbackLang, voice])

    useEffect(() => {
        sessionPhaseRef.current = sessionPhase
    }, [sessionPhase])

    const setStatus = useCallback((message) => {
        if (statusRef.current) {
            statusRef.current(message)
        }
    }, [])

    const playTurnAudio = useCallback((turnId) => {
        const chunks = ttsChunksRef.current.get(turnId)
        if (!chunks || !chunks.length) {
            return
        }
        ttsChunksRef.current.delete(turnId)

        const mergedPcm = mergeAudioChunks(chunks)
        if (!mergedPcm.length) {
            return
        }
        const wavBytes = pcm16ToWav(mergedPcm, 24000, 1)
        const audioBlob = new Blob([wavBytes], {type: 'audio/wav'})
        const url = URL.createObjectURL(audioBlob)
        const audio = new Audio(url)
        audio.play().catch(() => {
        })
        audio.onended = () => URL.revokeObjectURL(url)
    }, [])

    const ensureRealtimeConnected = useCallback(async () => {
        const existingSocketId = realtimeSocketIdRef.current
        const connectedModels = connectedRealtimeModelsRef.current
        const desiredConversationModel = settingsRef.current.realtimeConversationModel
        const desiredSttModel = settingsRef.current.realtimeSttModel

        if (existingSocketId) {
            const noModelBoundYet = !connectedModels.conversationModel && !connectedModels.sttModel
            if (noModelBoundYet) {
                connectedRealtimeModelsRef.current = {
                    conversationModel: desiredConversationModel,
                    sttModel: desiredSttModel
                }
                return existingSocketId
            }
            if (
                connectedModels.conversationModel === desiredConversationModel &&
                connectedModels.sttModel === desiredSttModel
            ) {
                return existingSocketId
            }
        }

        if (existingSocketId) {
            await closeRealtime(existingSocketId).catch(() => {
            })
            realtimeSocketIdRef.current = null
            setRealtimeConnected(false)
        }

        const openAiKey = await getApiKey('openai')
        if (!openAiKey) {
            throw new Error('실시간 음성을 사용하려면 OpenAI API Key가 필요합니다.')
        }

        const socketId = await connectRealtime(
            sessionId,
            openAiKey,
            desiredConversationModel,
            desiredSttModel
        )

        realtimeSocketIdRef.current = socketId
        connectedRealtimeModelsRef.current = {
            conversationModel: desiredConversationModel,
            sttModel: desiredSttModel
        }

        return socketId
    }, [sessionId])

    const syncRealtimeSettings = useCallback(async () => {
        if (!realtimeSocketIdRef.current) {
            return
        }

        const socketId = await ensureRealtimeConnected()
        const feedbackApiKey = await getApiKey(settingsRef.current.provider)

        await sendRealtime(socketId, {
            type: 'session.update',
            data: {
                conversationModel: settingsRef.current.realtimeConversationModel,
                sttModel: settingsRef.current.realtimeSttModel,
                feedbackProvider: settingsRef.current.provider,
                feedbackModel: settingsRef.current.feedbackModel,
                feedbackApiKey: feedbackApiKey || '',
                feedbackLanguage: settingsRef.current.feedbackLang,
                ttsVoice: settingsRef.current.voice
            }
        })
    }, [ensureRealtimeConnected])

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
    }, [refreshAudioDeviceStatus, setStatus])

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
            ignoreIncomingRef.current = false
            localStopRef.current = false

            const socketId = await ensureRealtimeConnected()
            await syncRealtimeSettings()

            const stream = await navigator.mediaDevices.getUserMedia({audio: true})
            const AudioContextClass = window.AudioContext || window.webkitAudioContext
            const audioContext = new AudioContextClass({sampleRate: 16000})
            const sourceNode = audioContext.createMediaStreamSource(stream)
            const processor = audioContext.createScriptProcessor(AUDIO_PROCESS_BUFFER_SIZE, 1, 1)

            processor.onaudioprocess = (event) => {
                if (!sessionActiveRef.current || realtimeSocketIdRef.current !== socketId) {
                    return
                }
                const channel = event.inputBuffer.getChannelData(0)
                const pcm = float32ToInt16(channel)
                const bytes = new Uint8Array(pcm.buffer)
                const base64Audio = toBase64(bytes)
                sendRealtime(socketId, {
                    type: 'audio.append',
                    data: {audio: base64Audio}
                }).catch(() => {
                })
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
            setStatus('세션을 시작했습니다. 모델이 첫 질문을 준비 중입니다.')
            refreshAudioDeviceStatus().catch(() => {
            })
        } catch (error) {
            setSessionPhase('IDLE')
            if (error?.name === 'NotAllowedError' || error?.name === 'SecurityError') {
                setAudioPermission('denied')
                setStatus('마이크 권한이 거부되었습니다. 아래에서 권한 요청을 다시 실행해 주세요.')
            } else if (error?.name === 'NotFoundError') {
                setStatus('사용 가능한 마이크 장치를 찾지 못했습니다.')
            } else {
                setStatus(error?.message || '세션을 시작하지 못했습니다.')
            }
            refreshAudioDeviceStatus().catch(() => {
            })
        }
    }, [ensureRealtimeConnected, syncRealtimeSettings, refreshAudioDeviceStatus, setStatus])

    const stopSession = useCallback(async ({
                                               forced = true,
                                               reason = 'user_stop',
                                               statusMessage = '세션을 종료했습니다.'
                                           } = {}) => {
        if (!sessionActiveRef.current && !processorRef.current && !streamRef.current && !realtimeSocketIdRef.current) {
            return
        }

        localStopRef.current = true
        ignoreIncomingRef.current = forced
        setSessionPhase('STOPPING')

        setSessionActive(false)
        sessionActiveRef.current = false
        stopCapture('')

        const socketId = realtimeSocketIdRef.current
        if (socketId) {
            if (forced) {
                await sendRealtime(socketId, {
                    type: 'session.stop',
                    data: {
                        forced: true,
                        reason
                    }
                }).catch(() => {
                })
            }
            await closeRealtime(socketId).catch(() => {
            })
        }

        realtimeSocketIdRef.current = null
        connectedRealtimeModelsRef.current = {conversationModel: '', sttModel: ''}
        setRealtimeConnected(false)
        setSessionPhase('IDLE')

        if (statusMessage) {
            setStatus(statusMessage)
        }
    }, [setStatus, stopCapture])

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

    useEffect(() => {
        sessionActiveRef.current = sessionActive
    }, [sessionActive])

    useEffect(() => {
        const unsubscribe = subscribeRealtime((event) => {
            if (!event) {
                return
            }

            if (!realtimeSocketIdRef.current && sessionPhaseRef.current === 'STARTING' && event.socketId) {
                // OpenAI websocket can emit first events before connect promise resolves.
                realtimeSocketIdRef.current = event.socketId
            }

            if (event.socketId !== realtimeSocketIdRef.current) {
                return
            }

            if (event.type === 'open') {
                setRealtimeConnected(true)
                syncRealtimeSettings().catch(() => {
                })
                return
            }

            if (event.type === 'close') {
                const reason = event.data?.reason ? ` (${event.data.reason})` : ''

                stopCapture('')
                setSessionActive(false)
                sessionActiveRef.current = false
                setRealtimeConnected(false)
                setSessionPhase('IDLE')
                realtimeSocketIdRef.current = null
                connectedRealtimeModelsRef.current = {conversationModel: '', sttModel: ''}

                if (!localStopRef.current) {
                    if (!serverStopRef.current) {
                        setStatus(`실시간 연결이 종료되어 세션을 중단했습니다.${reason}`)
                    }
                }
                serverStopRef.current = false
                localStopRef.current = false
                return
            }

            if (event.type === 'error') {
                const msg = typeof event.data === 'string' ? event.data : event.data?.message
                setStatus(`실시간 오류: ${msg || '알 수 없는 오류'}`)
                return
            }

            if (event.type !== 'message' || ignoreIncomingRef.current) {
                return
            }

            try {
                const {type, data} = parseRealtimeEnvelope(event.data)

                if (type === 'stt.partial') {
                    setPartialTranscript(data.text || '')
                    return
                }

                if (type === 'stt.final') {
                    setPartialTranscript('')
                    setTranscript(data.text || '')
                    setUserText(data.text || '')
                    return
                }

                if (type === 'question.prompt') {
                    const hasStructuredQuestion = data && Object.prototype.hasOwnProperty.call(data, 'question')
                    const questionNode = hasStructuredQuestion ? data?.question : data
                    const selection = data?.selection || {}
                    const normalizedQuestion = {
                        questionId: questionNode?.id || questionNode?.questionId || '',
                        text: questionNode?.text || '',
                        group: questionNode?.group || '',
                        exhausted: Boolean(selection?.exhausted),
                        selectionReason: selection?.reason || '',
                        mockExamCompleted: Boolean(selection?.exhausted && selection?.mode === 'MOCK_EXAM')
                    }
                    if (questionPromptRef.current) {
                        questionPromptRef.current(normalizedQuestion)
                    }
                    if (normalizedQuestion.text) {
                        setStatus('질문이 도착했습니다. 답변을 시작해 주세요.')
                    }
                    return
                }

                if (type === 'feedback.final') {
                    if (feedbackRef.current) {
                        feedbackRef.current(buildFeedbackFromRealtime(data))
                    }
                    if (refreshWrongNotesRef.current) {
                        refreshWrongNotesRef.current().catch(() => {
                        })
                    }
                    return
                }

                if (type === 'session.stopped') {
                    serverStopRef.current = true
                    const reason = data?.reason || ''
                    if (reason === 'question_exhausted' || reason === 'QUESTION_EXHAUSTED') {
                        setStatus('모든 질문을 완료하여 세션이 자동 종료되었습니다.')
                    } else {
                        setStatus('세션이 서버에서 종료되었습니다.')
                    }
                    return
                }

                if (type === 'tts.error') {
                    const msg = typeof data === 'string' ? data : data.message
                    setStatus(`음성 출력 오류: ${msg || '알 수 없는 오류'}`)
                    return
                }

                if (type === 'tts.chunk') {
                    const turnId = data.turnId
                    const existing = ttsChunksRef.current.get(turnId) || []
                    existing.push(data.audio)
                    ttsChunksRef.current.set(turnId, existing)
                    return
                }

                if (type === 'turn.completed') {
                    playTurnAudio(data.turnId)
                    return
                }

                if (type === 'error') {
                    const msg = typeof data === 'string' ? data : data.message
                    setStatus(`오류: ${msg || '알 수 없는 오류'}`)
                }
            } catch (_error) {
                setStatus('실시간 이벤트를 처리하지 못했습니다.')
            }
        })

        return () => {
            if (unsubscribe) {
                unsubscribe()
            }
        }
    }, [playTurnAudio, setStatus, stopCapture, syncRealtimeSettings])

    useEffect(() => {
        return () => {
            stopSession({forced: false, statusMessage: ''}).catch(() => {
            })
        }
    }, [stopSession])

    useEffect(() => {
        if (!realtimeConnected) {
            return
        }
        syncRealtimeSettings().catch(() => {
        })
    }, [
        provider,
        feedbackModel,
        realtimeConversationModel,
        realtimeSttModel,
        feedbackLang,
        voice,
        realtimeConnected,
        syncRealtimeSettings
    ])

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
        recording: sessionActive,
        sessionActive,
        sessionPhase,
        realtimeConnected,
        partialTranscript,
        transcript,
        userText,
        setUserText,
        audioPermission,
        audioDeviceStatus,
        startSession,
        stopSession,
        startRecording: startSession,
        stopRecording: stopSession,
        syncRealtimeSettings,
        refreshAudioDeviceStatus,
        handleAudioQuickAction
    }
}
