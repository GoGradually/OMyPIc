export function parseMockGroupCounts(rawValue) {
    try {
        return JSON.parse(rawValue || '{}')
    } catch (_error) {
        return {}
    }
}

export function buildModePayload({sessionId, listId, mode, batchSize, mockOrder, mockCounts}) {
    return {
        sessionId,
        listId: listId || null,
        mode,
        continuousBatchSize: batchSize,
        mockGroupOrder: mockOrder.split(',').map((item) => item.trim()).filter(Boolean),
        mockGroupCounts: parseMockGroupCounts(mockCounts)
    }
}

export function getCurrentQuestionLabel(currentQuestion) {
    if (!currentQuestion) {
        return '아직 선택된 질문이 없습니다. “다음 질문”을 눌러 시작하세요.'
    }
    if (currentQuestion.mockExamCompleted) {
        return '모든 모의고사 질문을 완료했습니다.'
    }
    return currentQuestion.text || '질문을 불러오지 못했습니다.'
}
