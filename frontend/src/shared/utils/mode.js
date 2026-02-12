export function buildModePayload({sessionId, listId, mode, batchSize}) {
    return {
        sessionId,
        listId: listId || null,
        mode,
        continuousBatchSize: batchSize
    }
}

export function getCurrentQuestionLabel(currentQuestion) {
    if (!currentQuestion) {
        return '세션 시작을 누르면 첫 질문이 자동으로 제시됩니다.'
    }
    if (currentQuestion.exhausted) {
        return '모든 질문을 완료했습니다.'
    }
    return currentQuestion.text || '질문을 불러오지 못했습니다.'
}
