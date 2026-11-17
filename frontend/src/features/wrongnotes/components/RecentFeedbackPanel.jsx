import React from 'react'

const CORRECTION_LABELS = [
    {key: 'grammar', label: '문법'},
    {key: 'expression', label: '표현'},
    {key: 'logic', label: '논리'}
]

const RECOMMENDATION_LABELS = [
    {key: 'filler', label: '필러'},
    {key: 'adjective', label: '형용사'},
    {key: 'adverb', label: '부사'}
]

function pickCorrections(corrections = {}) {
    return CORRECTION_LABELS
        .map(({key, label}) => ({
            label,
            issue: corrections[key]?.issue || '',
            fix: corrections[key]?.fix || ''
        }))
        .filter((item) => item.issue || item.fix)
}

function pickRecommendations(recommendations = {}) {
    return RECOMMENDATION_LABELS
        .map(({key, label}) => ({
            label,
            term: recommendations[key]?.term || '',
            usage: recommendations[key]?.usage || ''
        }))
        .filter((item) => item.term || item.usage)
}

function FeedbackItem({item}) {
    const corrections = pickCorrections(item.corrections)
    const recommendations = pickRecommendations(item.recommendations)

    return (
        <article className="markup">
            {item.questionText && <p className="markup__question" lang="en">{item.questionText}</p>}
            {item.summary && <p className="markup__summary">{item.summary}</p>}

            {corrections.length > 0 && (
                <>
                    <h4>교정 포인트</h4>
                    <dl className="markup__list">
                        {corrections.map((correction) => (
                            <div key={correction.label}>
                                <dt>{correction.label}</dt>
                                <dd>
                                    {correction.issue && <span className="markup__issue">{correction.issue}</span>}
                                    {correction.fix && <span className="markup__fix">{correction.fix}</span>}
                                </dd>
                            </div>
                        ))}
                    </dl>
                </>
            )}

            {recommendations.length > 0 && (
                <>
                    <h4>추천 표현</h4>
                    <dl className="markup__list">
                        {recommendations.map((recommendation) => (
                            <div key={recommendation.label}>
                                <dt>{recommendation.label}</dt>
                                <dd>
                                    {recommendation.term && <mark lang="en">{recommendation.term}</mark>}
                                    {recommendation.usage && <span>{recommendation.usage}</span>}
                                </dd>
                            </div>
                        ))}
                    </dl>
                </>
            )}
        </article>
    )
}

export function RecentFeedbackPanel({feedback, aiFeedbackEnabled = true, onOpenWrongnotes}) {
    const items = feedback?.items && feedback.items.length > 0 ? feedback.items : (feedback ? [feedback] : [])

    return (
        <section className="side-section recent-feedback-panel">
            <div className="section-head">
                <h3>최근 피드백</h3>
                <button className="control-button secondary" onClick={onOpenWrongnotes}>
                    오답노트
                </button>
            </div>

            {items.length === 0 && (
                <p className="empty-text">
                    {aiFeedbackEnabled
                        ? '답변을 마치면 AI 첨삭이 여기에 표시됩니다.'
                        : 'AI 피드백이 꺼져 있습니다. 켜면 답변마다 첨삭을 받을 수 있습니다.'}
                </p>
            )}
            {items.length > 1 && <p className="tiny-meta">문답별 피드백 {items.length}건</p>}
            {items.map((item, index) => (
                <FeedbackItem key={`${item.questionId || 'item'}-${index}`} item={item}/>
            ))}
        </section>
    )
}
