export function parseRealtimeEnvelope(raw) {
    const envelope = JSON.parse(raw)
    return {
        type: envelope?.type || '',
        data: envelope?.data || {}
    }
}

export function buildFeedbackFromRealtime(data) {
    return {
        summary: data.summary || '',
        correctionPoints: data.correctionPoints || [],
        exampleAnswer: data.exampleAnswer || '',
        rulebookEvidence: data.rulebookEvidence || []
    }
}
