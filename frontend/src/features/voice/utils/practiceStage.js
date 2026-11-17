// The practice turn is a real sequence, so the stage is shown as numbered steps:
//   AI feedback on (immediate) : 질문 듣기 → 답변 → 피드백
//   AI feedback on (continuous): 질문 듣기 → 답변   (feedback arrives once per question group)
//   AI feedback off            : 질문 듣기 → 답변 → 내 응답 다시듣기 (when auto replay is on)

export function getPracticeSteps({feedbackEnabled, autoReplayEnabled, continuousMode}) {
    const steps = ['질문 듣기', '답변']
    if (feedbackEnabled) {
        if (!continuousMode) {
            steps.push('피드백')
        }
        return steps
    }
    if (autoReplayEnabled) {
        steps.push('내 응답 다시듣기')
    }
    return steps
}

export function getActiveStepIndex({sessionActive, speechState, replayPlaying, awaitingReview, stepCount}) {
    if (!sessionActive) {
        return -1
    }
    const lastStep = stepCount - 1
    if (replayPlaying || speechState === 'PLAYING_FEEDBACK_TTS') {
        return lastStep
    }
    if (speechState === 'READY_FOR_ANSWER' || speechState === 'CAPTURING_ANSWER') {
        return 1
    }
    if (speechState === 'PLAYING_QUESTION_TTS') {
        return 0
    }
    if (speechState === 'WAITING_TTS') {
        return awaitingReview && stepCount > 2 ? lastStep : 0
    }
    return -1
}

export function getIdleCaption({feedbackEnabled, autoReplayEnabled, continuousMode}) {
    if (feedbackEnabled) {
        return continuousMode
            ? '질문 그룹이 끝날 때마다 AI가 한 번에 첨삭합니다.'
            : '답변을 마칠 때마다 AI가 첨삭합니다.'
    }
    return autoReplayEnabled
        ? 'AI 없이 질문 듣기, 답변, 내 응답 다시듣기가 자동으로 이어집니다.'
        : 'AI 없이 질문 듣기와 답변만 자동으로 이어집니다.'
}

export function getStageCaption({
                                    sessionActive,
                                    speechState,
                                    replayPlaying,
                                    awaitingReview,
                                    feedbackEnabled,
                                    autoReplayEnabled,
                                    continuousMode
                                }) {
    if (!sessionActive) {
        return getIdleCaption({feedbackEnabled, autoReplayEnabled, continuousMode})
    }
    if (replayPlaying) {
        return '내 응답을 다시 듣는 중입니다.'
    }
    switch (speechState) {
        case 'READY_FOR_ANSWER':
            return '지금 답변해 주세요.'
        case 'CAPTURING_ANSWER':
            return '답변을 듣고 있습니다. 말을 멈추면 3초 뒤 자동으로 넘어갑니다.'
        case 'PLAYING_QUESTION_TTS':
            return '질문을 읽어 드립니다. 두 번 재생됩니다.'
        case 'PLAYING_FEEDBACK_TTS':
            return '피드백을 읽어 드립니다.'
        case 'RECOVERING':
            return '연결을 복구하고 있습니다.'
        case 'STOPPING':
            return '세션을 정리하고 있습니다.'
        case 'WAITING_TTS':
            if (awaitingReview && feedbackEnabled && !continuousMode) {
                return 'AI가 답변을 첨삭하고 있습니다.'
            }
            return awaitingReview ? '다음 질문을 준비하고 있습니다.' : '질문을 준비하고 있습니다.'
        default:
            return '세션을 진행하고 있습니다.'
    }
}
