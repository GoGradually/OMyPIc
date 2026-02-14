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
                                    localStopRef,
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
                                    enqueueTtsAudio,
                                    handleTtsFailure
                                }) {
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
        const normalizedQuestion = parseQuestionPayload(data)
        resetPendingAudio()
        updateSpeechState(speechState.WAITING_TTS)
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
        const data = parseSseData(event?.data)
        onFeedback?.(buildFeedbackFromVoice(data))
        refreshWrongNotes?.().catch(() => {
        })
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
}
