export function parseRealtimeEnvelope(raw) {
    const envelope = JSON.parse(raw)
    return {
        type: envelope?.type || '',
        data: envelope?.data || {}
    }
}

function normalizeItem(item = {}) {
    return {
        questionId: item.questionId || '',
        questionText: item.questionText || '',
        questionGroup: item.questionGroup || '',
        answerText: item.answerText || '',
        summary: item.summary || '',
        correctionPoints: item.correctionPoints || [],
        exampleAnswer: item.exampleAnswer || '',
        rulebookEvidence: item.rulebookEvidence || []
    }
}

export function buildFeedbackFromRealtime(data) {
    const batchItems = Array.isArray(data?.batch?.items) ? data.batch.items : []
    const legacyItems = Array.isArray(data?.items) ? data.items : []
    const sourceItems = batchItems.length > 0 ? batchItems : legacyItems
    const items = sourceItems.length > 0 ? sourceItems.map((item) => normalizeItem(item)) : [normalizeItem(data)]
    const primary = items[0] || normalizeItem()

    return {
        mode: data?.policy?.mode || data?.mode || '',
        batch: {
            size: Number.isInteger(data?.batch?.size) ? data.batch.size : items.length,
            isResidual: Boolean(data?.batch?.isResidual),
            reason: data?.policy?.reason || ''
        },
        nextAction: {
            type: data?.nextAction?.type || '',
            reason: data?.nextAction?.reason || ''
        },
        items,
        summary: primary.summary,
        correctionPoints: primary.correctionPoints,
        exampleAnswer: primary.exampleAnswer,
        rulebookEvidence: primary.rulebookEvidence
    }
}
