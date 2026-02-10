import React from 'react'

export function QuestionPanel({currentQuestionLabel, modeSummary, onOpenQuestionsPanel}) {
    return (
        <section className="panel question-panel">
            <div className="panel-head question-panel__head">
                <h3>현재 질문</h3>
                <span className="tiny-meta">세션 진행 중 자동 전환</span>
            </div>

            <div className="current-question-box question-panel__current">{currentQuestionLabel}</div>

            <div className="mode-summary question-panel__mode-summary">
                <div>
                    <p className="mode-label">학습 모드</p>
                    <strong>{modeSummary}</strong>
                </div>
                <button
                    className="control-button secondary"
                    onClick={onOpenQuestionsPanel}
                >
                    모드/질문 관리
                </button>
            </div>
        </section>
    )
}
