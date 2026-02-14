export function parseVoiceEnvelope(raw) {
    const envelope = JSON.parse(raw)
    return {
        type: envelope?.type || '',
        data: envelope?.data || {}
    }
}

function normalizeCorrection(detail = {}) {
    return {
        issue: detail.issue || '',
        fix: detail.fix || ''
    }
}

function normalizeRecommendation(detail = {}) {
    return {
        term: detail.term || '',
        usage: detail.usage || ''
    }
}

function normalizeItem(item = {}) {
    const corrections = item.corrections || {}
    const recommendations = item.recommendations || {}
    return {
        questionId: item.questionId || '',
        questionText: item.questionText || '',
        questionGroup: item.questionGroup || '',
        answerText: item.answerText || '',
        summary: item.summary || '',
        corrections: {
            grammar: normalizeCorrection(corrections.grammar),
            expression: normalizeCorrection(corrections.expression),
            logic: normalizeCorrection(corrections.logic)
        },
        recommendations: {
            filler: normalizeRecommendation(recommendations.filler),
            adjective: normalizeRecommendation(recommendations.adjective),
            adverb: normalizeRecommendation(recommendations.adverb)
        },
        exampleAnswer: item.exampleAnswer || '',
        rulebookEvidence: item.rulebookEvidence || []
    }
}

export function buildFeedbackFromVoice(data) {
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
            reason: data?.policy?.reason || '',
            groupBatchSize: Number.isInteger(data?.policy?.groupBatchSize)
                ? data.policy.groupBatchSize
                : (Number.isInteger(data?.policy?.batchSize) ? data.policy.batchSize : 1)
        },
        nextAction: {
            type: data?.nextAction?.type || '',
            reason: data?.nextAction?.reason || ''
        },
        items,
        summary: primary.summary,
        corrections: primary.corrections,
        recommendations: primary.recommendations,
        exampleAnswer: primary.exampleAnswer,
        rulebookEvidence: primary.rulebookEvidence
    }
}
