import {useCallback, useRef} from 'react'
import {fromBase64} from '../../../shared/utils/audioCodec.js'
import {prepareLocalSpeech, speakLocally} from '../local/localSpeech.js'

const QUESTION_REPEAT_DELAY_MS = 3000

/**
 * Plays session speech in order. An item is either audio synthesized by the server (tts.audio)
 * or text read aloud on this device (tts.text). Questions play twice, 3 seconds apart, and the
 * answer window opens after the second pass.
 */
export function useTtsPlaybackQueue({
                                        resetPendingAudio,
                                        updateSpeechState,
                                        onQuestionPlaybackCompleted,
                                        onTtsFailure,
                                        onDeviceVoiceUnavailable,
                                        onLocalSpeechNotice,
                                        getSpeechEngine,
                                        speechState
                                    }) {
    const ttsPlaybackQueueRef = useRef([])
    const ttsPlayingRef = useRef(false)
    const ttsActiveAudioRef = useRef(null)
    const ttsCancelSpeechRef = useRef(null)
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
        if (ttsCancelSpeechRef.current) {
            ttsCancelSpeechRef.current()
            ttsCancelSpeechRef.current = null
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

        const completeItem = () => {
            ttsPlayingRef.current = false
            if (nextItem.role === 'question') {
                if (nextItem.isRepeat) {
                    onQuestionPlaybackCompleted()
                }
            } else {
                updateSpeechState(speechState.WAITING_TTS)
            }
            playNextTtsAudio()
        }

        if (nextItem.speechText) {
            ttsPlayingRef.current = true
            ttsCancelSpeechRef.current = speakLocally(nextItem.speechText, {
                engine: getSpeechEngine?.(),
                onNotice: onLocalSpeechNotice,
                onEnd: () => {
                    ttsCancelSpeechRef.current = null
                    completeItem()
                },
                onUnavailable: () => {
                    // The text is on screen, so the session goes on without the voice.
                    ttsCancelSpeechRef.current = null
                    onDeviceVoiceUnavailable?.()
                    completeItem()
                }
            })
            return
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
            URL.revokeObjectURL(url)
            if (failed) {
                ttsPlayingRef.current = false
                onTtsFailure('TTS 출력 실패로 세션을 종료했습니다.')
                return
            }
            completeItem()
        }

        audio.onended = () => finishPlayback()
        audio.onerror = () => finishPlayback({failed: true})
        audio.play().catch(() => {
            finishPlayback({failed: true})
        })
    }, [
        getSpeechEngine,
        onDeviceVoiceUnavailable,
        onLocalSpeechNotice,
        onQuestionPlaybackCompleted,
        onTtsFailure,
        resetPendingAudio,
        speechState,
        updateSpeechState
    ])

    const enqueueTtsAudio = useCallback((data) => {
        const audioBase64 = data?.audio || ''
        // tts.text events carry text for the device voice; tts.audio events also carry text, as a caption.
        const speechText = data?.deviceVoice ? (data?.text || '').trim() : ''
        if (!audioBase64 && !speechText) {
            return
        }
        if (speechText) {
            // Synthesis can take a while; start now so it is ready when this item's turn comes.
            prepareLocalSpeech(speechText, getSpeechEngine?.()).catch(() => {
            })
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
            speechText,
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
    }, [getSpeechEngine, playNextTtsAudio])

    return {
        clearTtsPlayback,
        enqueueTtsAudio
    }
}
