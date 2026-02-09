import {useCallback, useEffect, useRef, useState} from 'react'
import {closeRealtime, connectRealtime, getApiKey, sendRealtime, subscribeRealtime} from '../../../shared/api/http.js'
import {float32ToInt16, mergeAudioChunks, toBase64} from '../../../shared/utils/audioCodec.js'
import {buildFeedbackFromRealtime, parseRealtimeEnvelope} from '../utils/realtimeEvent.js'

const INITIAL_AUDIO_DEVICE_STATUS = {
    inputCount: 0,
    outputCount: 0,
    activeInputLabel: '',
    liveInput: false,
    lastCheckedAt: ''
}

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
                                       refreshWrongNotes
                                   }) {
    const [recording, setRecording] = useState(false)
    const [realtimeConnected, setRealtimeConnected] = useState(false)
    const [partialTranscript, setPartialTranscript] = useState('')
    const [transcript, setTranscript] = useState('')
    const [userText, setUserText] = useState('')
    const [audioPermission, setAudioPermission] = useState('unknown')
    const [audioDeviceStatus, setAudioDeviceStatus] = useState(INITIAL_AUDIO_DEVICE_STATUS)

    const statusRef = useRef(onStatus)
    const feedbackRef = useRef(onFeedback)
    const refreshWrongNotesRef = useRef(refreshWrongNotes)

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

    const recordingRef = useRef(false)
    const streamRef = useRef(null)
    const audioContextRef = useRef(null)
    const sourceNodeRef = useRef(null)
    const processorRef = useRef(null)
    const recordingTimeoutRef = useRef(null)

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
        settingsRef.current = {
            provider,
            feedbackModel,
            realtimeConversationModel,
            realtimeSttModel,
            feedbackLang,
            voice
        }
    }, [provider, feedbackModel, realtimeConversationModel, realtimeSttModel, feedbackLang, voice])

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

        const merged = mergeAudioChunks(chunks)
        const audioBlob = new Blob([merged], {type: 'audio/mpeg'})
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

        if (
            existingSocketId &&
            connectedModels.conversationModel === desiredConversationModel &&
            connectedModels.sttModel === desiredSttModel
        ) {
            return existingSocketId
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

        if (requestPermission && !recordingRef.current) {
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

    const stopRecording = useCallback(() => {
        if (!recordingRef.current && !processorRef.current && !streamRef.current) {
            return
        }

        setRecording(false)
        recordingRef.current = false

        if (recordingTimeoutRef.current) {
            clearTimeout(recordingTimeoutRef.current)
            recordingTimeoutRef.current = null
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

        const socketId = realtimeSocketIdRef.current
        if (socketId) {
            sendRealtime(socketId, {type: 'audio.commit'}).catch(() => {
            })
        }

        setStatus('녹음이 종료되어 피드백을 생성 중입니다.')
        refreshAudioDeviceStatus().catch(() => {
        })
    }, [refreshAudioDeviceStatus, setStatus])

    const startRecording = useCallback(async () => {
        if (recordingRef.current) {
            return
        }

        if (!navigator.mediaDevices || !navigator.mediaDevices.getUserMedia) {
            setAudioPermission('unsupported')
            setStatus('이 환경에서는 오디오 입력 장치를 지원하지 않습니다.')
            return
        }

        try {
            const socketId = await ensureRealtimeConnected()
            await syncRealtimeSettings()

            const stream = await navigator.mediaDevices.getUserMedia({audio: true})
            const AudioContextClass = window.AudioContext || window.webkitAudioContext
            const audioContext = new AudioContextClass({sampleRate: 16000})
            const sourceNode = audioContext.createMediaStreamSource(stream)
            const processor = audioContext.createScriptProcessor(4096, 1, 1)

            processor.onaudioprocess = (event) => {
                if (!recordingRef.current) {
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

            setRecording(true)
            recordingRef.current = true
            setAudioPermission('granted')
            setStatus('녹음을 시작했습니다.')
            refreshAudioDeviceStatus().catch(() => {
            })

            recordingTimeoutRef.current = setTimeout(() => {
                stopRecording()
            }, 180000)
        } catch (error) {
            if (error?.name === 'NotAllowedError' || error?.name === 'SecurityError') {
                setAudioPermission('denied')
                setStatus('마이크 권한이 거부되었습니다. 아래에서 권한 요청을 다시 실행해 주세요.')
            } else if (error?.name === 'NotFoundError') {
                setStatus('사용 가능한 마이크 장치를 찾지 못했습니다.')
            } else {
                setStatus(error?.message || '녹음을 시작하지 못했습니다.')
            }
            refreshAudioDeviceStatus().catch(() => {
            })
        }
    }, [ensureRealtimeConnected, syncRealtimeSettings, refreshAudioDeviceStatus, stopRecording, setStatus])

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
        recordingRef.current = recording
    }, [recording])

    useEffect(() => {
        const unsubscribe = subscribeRealtime((event) => {
            if (!event || event.socketId !== realtimeSocketIdRef.current) {
                return
            }

            if (event.type === 'open') {
                setRealtimeConnected(true)
                syncRealtimeSettings().catch(() => {
                })
                return
            }

            if (event.type === 'close') {
                setRealtimeConnected(false)
                realtimeSocketIdRef.current = null
                connectedRealtimeModelsRef.current = {conversationModel: '', sttModel: ''}
                return
            }

            if (event.type === 'error') {
                setStatus(`실시간 오류: ${event.data || '알 수 없는 오류'}`)
                return
            }

            if (event.type !== 'message') {
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
    }, [playTurnAudio, setStatus, syncRealtimeSettings])

    useEffect(() => {
        return () => {
            stopRecording()
            const socketId = realtimeSocketIdRef.current
            if (socketId) {
                closeRealtime(socketId).catch(() => {
                })
            }
        }
    }, [stopRecording])

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
        recording,
        realtimeConnected,
        partialTranscript,
        transcript,
        userText,
        setUserText,
        audioPermission,
        audioDeviceStatus,
        startRecording,
        stopRecording,
        syncRealtimeSettings,
        refreshAudioDeviceStatus,
        handleAudioQuickAction
    }
}
