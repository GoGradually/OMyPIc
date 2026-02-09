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
                    <p className="feedback-summary">{feedback.summary}</p>
                    <ul className="bullet-list">
                        {feedback.correctionPoints.map((point, idx) => (
                            <li key={idx}>{point}</li>
                        ))}
                    </ul>
                </>
            )}
        </section>
    )
}
