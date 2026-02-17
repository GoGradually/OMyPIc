import {buildFeedbackFromVoice} from '../utils/voiceEvent.js'
import {parseSseData} from './voiceSessionAudio.js'

function parseQuestionPayload(data) {
    const hasStructuredQuestion = data && Object.prototype.hasOwnProperty.call(data, 'question')
    const questionNode = hasStructuredQuestion ? data?.question : data
    const selection = data?.selection || {}
    return {
        questionId: questionNode?.id || questionNode?.questionId || '',
        text: questionNode?.text || '',
        group: questionNode?.group || '',
        exhausted: Boolean(selection?.exhausted),
        selectionReason: selection?.reason || ''
    }
}

export function bindVoiceEvents({
                                    eventSource,
                                    serverStopRef,
                                    setVoiceConnected,
                                    setStatus,
                                    setPartialTranscript,
                                    setTranscript,
                                    setUserText,
                                    resetPendingAudio,
                                    updateSpeechState,
                                    speechState,
                                    onQuestionPrompt,
                                    onFeedback,
                                    refreshWrongNotes,
                                    stopSession,
                                    shouldResumeImmediatelyOnQuestionPrompt,
                                    enqueueTtsAudio,
                                    handleTtsFailure,
                                    onSttSkipped,
                                    shouldProcessEvent,
                                    shouldSkipReplayTts,
                                    onConnectionError
                                }) {
    const processData = (event) => {
        const data = parseSseData(event?.data)
        if (shouldProcessEvent && !shouldProcessEvent(data)) {
            return null
        }
        return data
    }

    eventSource.addEventListener('session.ready', (event) => {
        const data = processData(event)
        if (!data) {
            return
        }
        setVoiceConnected(true)
        setStatus('음성 세션에 연결되었습니다.')
    })

    eventSource.addEventListener('stt.final', (event) => {
        const data = processData(event)
        if (!data) {
            return
        }
        const text = data?.text || ''
        setPartialTranscript('')
        setTranscript(text)
        setUserText(text)
    })

    eventSource.addEventListener('stt.skipped', (event) => {
        const data = processData(event)
        if (!data) {
            return
        }
        onSttSkipped?.(data)
        setStatus('음성이 인식되지 않아 다시 답변해 주세요.')
    })

    eventSource.addEventListener('question.prompt', (event) => {
        const data = processData(event)
        if (!data) {
            return
        }
        const normalizedQuestion = parseQuestionPayload(data)
        resetPendingAudio()
        if (shouldResumeImmediatelyOnQuestionPrompt?.(data)) {
            updateSpeechState(speechState.READY_FOR_ANSWER)
        } else {
            updateSpeechState(speechState.WAITING_TTS)
        }
        onQuestionPrompt?.(normalizedQuestion)
        if (normalizedQuestion.exhausted) {
            setStatus('모든 질문을 완료했습니다.')
            return
        }
        if (normalizedQuestion.text) {
            setStatus('질문이 도착했습니다. 답변을 시작해 주세요.')
        }
    })

    eventSource.addEventListener('feedback.final', (event) => {
        const data = processData(event)
        if (!data) {
            return
        }
        onFeedback?.(buildFeedbackFromVoice(data))
        refreshWrongNotes?.().catch(() => {
        })
    })

    eventSource.addEventListener('session.stopped', (event) => {
        const data = processData(event)
        if (!data) {
            return
        }
        serverStopRef.current = true
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
        const data = processData(event)
        if (!data) {
            return
        }
        if (shouldSkipReplayTts?.(data)) {
            return
        }
        enqueueTtsAudio(data)
    })

    eventSource.addEventListener('tts.error', (event) => {
        const data = processData(event)
        if (!data) {
            return
        }
        const message = data?.message
            ? `TTS 출력 실패로 세션을 종료했습니다. (${data.message})`
            : 'TTS 출력 실패로 세션을 종료했습니다.'
        handleTtsFailure(message)
    })

    eventSource.addEventListener('session.error', (event) => {
        const data = processData(event)
        if (!data) {
            return
        }
        if (data?.message) {
            setStatus(`오류: ${data.message}`)
        }
    })

    eventSource.onerror = () => {
        onConnectionError?.()
    }
}
