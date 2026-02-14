import React from 'react'

function correctionLines(corrections = {}) {
    return [
        {label: 'Grammar', value: corrections.grammar},
        {label: 'Expression', value: corrections.expression},
        {label: 'Logic', value: corrections.logic}
    ]
        .map(({label, value}) => {
            const issue = value?.issue || ''
            const fix = value?.fix || ''
            if (!issue && !fix) {
                return ''
            }
            if (issue && fix) {
                return `${label}: ${issue} / ${fix}`
            }
            return `${label}: ${issue || fix}`
        })
        .filter(Boolean)
}

function recommendationLines(recommendations = {}) {
    return [
        {label: 'Filler', value: recommendations.filler},
        {label: 'Adjective', value: recommendations.adjective},
        {label: 'Adverb', value: recommendations.adverb}
    ]
        .map(({label, value}) => {
            const term = value?.term || ''
            const usage = value?.usage || ''
            if (!term && !usage) {
                return ''
            }
            if (term && usage) {
                return `${label}: ${term} - ${usage}`
            }
            return `${label}: ${term || usage}`
        })
        .filter(Boolean)
}

export function RecentFeedbackPanel({feedback, onOpenWrongnotes}) {
    return (
        <section className="panel compact-panel recent-feedback-panel">
            <div className="panel-head">
                <h3>최근 피드백</h3>
                <button className="control-button secondary" onClick={onOpenWrongnotes}>
                    오답노트
                </button>
            </div>
            {!feedback && <p className="empty-text">아직 생성된 피드백이 없습니다.</p>}
            {feedback && (
                <>
                    {feedback.items && feedback.items.length > 1 && (
                        <p className="tiny-meta">문답별 피드백 {feedback.items.length}건</p>
                    )}
                    {(feedback.items && feedback.items.length > 0 ? feedback.items : [feedback]).map((item, idx) => {
                        const corrections = correctionLines(item.corrections)
                        const recommendations = recommendationLines(item.recommendations)
                        return (
                            <div key={`${item.questionId || 'item'}-${idx}`} className="feedback-block">
                                {item.questionText && <div className="feedback-title">{item.questionText}</div>}
                                <p className="feedback-summary">{item.summary}</p>
                                {corrections.length > 0 && (
                                    <>
                                        <p className="tiny-meta">교정 포인트</p>
                                        <ul className="bullet-list">
                                            {corrections.map((point, pointIndex) => (
                                                <li key={`${idx}-correction-${pointIndex}`}>{point}</li>
                                            ))}
                                        </ul>
                                    </>
                                )}
                                {recommendations.length > 0 && (
                                    <>
                                        <p className="tiny-meta">추천 표현</p>
                                        <ul className="bullet-list">
                                            {recommendations.map((point, pointIndex) => (
                                                <li key={`${idx}-recommendation-${pointIndex}`}>{point}</li>
                                            ))}
                                        </ul>
                                    </>
                                )}
                            </div>
                        )
                    })}
                </>
            )}
        </section>
    )
}
