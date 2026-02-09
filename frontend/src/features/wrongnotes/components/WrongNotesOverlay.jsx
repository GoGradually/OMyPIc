import React from 'react'

export function WrongNotesOverlay({wrongNotes, feedback}) {
    return (
        <div className="overlay-content">
            <div className="count-banner">
                누적 피드백 <span>{wrongNotes.length}</span>건
            </div>

            <ul className="item-list wrongnote-items">
                {wrongNotes.map((note) => (
                    <li key={note.id}>
                        <div className="summary-line">{note.shortSummary}</div>
                        <div className="count-line">{note.pattern} · {note.count}회</div>
                    </li>
                ))}
            </ul>

            {wrongNotes.length === 0 && <p className="empty-text">아직 기록된 오답 노트가 없습니다.</p>}

            {feedback && (
                <div className="feedback-block">
                    <div className="feedback-title">최근 개선 예시 답변</div>
                    <p>{feedback.exampleAnswer}</p>
                    {feedback.rulebookEvidence && feedback.rulebookEvidence.length > 0 && (
                        <ul className="bullet-list">
                            {feedback.rulebookEvidence.map((item, idx) => (
                                <li key={idx}>{item}</li>
                            ))}
                        </ul>
                    )}
                </div>
            )}
        </div>
    )
}
