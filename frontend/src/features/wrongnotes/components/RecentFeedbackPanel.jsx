import React from 'react'

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
                    {(feedback.items && feedback.items.length > 0 ? feedback.items : [feedback]).map((item, idx) => (
                        <div key={`${item.questionId || 'item'}-${idx}`} className="feedback-block">
                            {item.questionText && <div className="feedback-title">{item.questionText}</div>}
                            <p className="feedback-summary">{item.summary}</p>
                            <ul className="bullet-list">
                                {(item.correctionPoints || []).map((point, pointIndex) => (
                                    <li key={`${idx}-${pointIndex}`}>{point}</li>
                                ))}
                            </ul>
                        </div>
                    ))}
                </>
            )}
        </section>
    )
}
