import {useCallback, useRef} from 'react'
import {fromBase64} from '../../../shared/utils/audioCodec.js'

const QUESTION_REPEAT_DELAY_MS = 3000

export function useTtsPlaybackQueue({
                                        resetPendingAudio,
                                        updateSpeechState,
                                        onQuestionPlaybackCompleted,
                                        onTtsFailure,
                                        speechState
                                    }) {
    const ttsPlaybackQueueRef = useRef([])
    const ttsPlayingRef = useRef(false)
    const ttsActiveAudioRef = useRef(null)
    const ttsDelayTimerRef = useRef(null)
    const ttsReceiveOrderRef = useRef(0)

    const clearTtsPlayback = useCallback(() => {
        ttsPlaybackQueueRef.current = []
        ttsPlayingRef.current = false
        ttsReceiveOrderRef.current = 0
        if (ttsDelayTimerRef.current !== null) {
            clearTimeout(ttsDelayTimerRef.current)
            ttsDelayTimerRef.current = null
        }
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

    const playNextTtsAudio = useCallback(() => {
        if (ttsPlayingRef.current) {
            return
        }
        const nextItem = ttsPlaybackQueueRef.current.shift()
        if (!nextItem) {
            return
        }

        if (nextItem.delayMs > 0) {
            ttsPlayingRef.current = true
            ttsDelayTimerRef.current = setTimeout(() => {
                ttsDelayTimerRef.current = null
                ttsPlayingRef.current = false
                ttsPlaybackQueueRef.current.unshift({
                    ...nextItem,
                    delayMs: 0
                })
                playNextTtsAudio()
            }, nextItem.delayMs)
            return
        }

        resetPendingAudio()
        if (nextItem.role === 'feedback') {
            updateSpeechState(speechState.PLAYING_FEEDBACK_TTS)
        } else {
            updateSpeechState(speechState.PLAYING_QUESTION_TTS)
        }

        const bytes = fromBase64(nextItem.audioBase64)
        if (!bytes.length) {
            onTtsFailure('TTS 출력 실패로 세션을 종료했습니다.')
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
                onTtsFailure('TTS 출력 실패로 세션을 종료했습니다.')
                return
            }
            if (nextItem.role === 'question') {
                if (nextItem.isRepeat) {
                    onQuestionPlaybackCompleted()
                }
            } else {
                updateSpeechState(speechState.WAITING_TTS)
            }
            playNextTtsAudio()
        }

        audio.onended = () => finishPlayback()
        audio.onerror = () => finishPlayback({failed: true})
        audio.play().catch(() => {
            finishPlayback({failed: true})
        })
    }, [onQuestionPlaybackCompleted, onTtsFailure, resetPendingAudio, speechState, updateSpeechState])

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
            mimeType: data?.mimeType || 'audio/wav',
            isRepeat: false,
            delayMs: 0
        }
        ttsPlaybackQueueRef.current.push(item)
        if (role === 'question') {
            ttsPlaybackQueueRef.current.push({
                ...item,
                order: ++ttsReceiveOrderRef.current,
                isRepeat: true,
                delayMs: QUESTION_REPEAT_DELAY_MS
            })
        }
        ttsPlaybackQueueRef.current.sort((left, right) => {
            const leftHasSequence = left.sequence !== null
            const rightHasSequence = right.sequence !== null
            if (leftHasSequence && rightHasSequence) {
                if (left.sequence !== right.sequence) {
                    return left.sequence - right.sequence
                }
                return left.order - right.order
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

    return {
        clearTtsPlayback,
        enqueueTtsAudio
    }
}
