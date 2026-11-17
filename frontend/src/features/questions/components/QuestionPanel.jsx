import React from 'react'

export function QuestionPanel({
                                  modeSummary,
                                  onOpenQuestionGroupManager,
                                  onOpenGroupQuestionManager,
                                  onOpenLearningMode
                              }) {
    return (
        <section className="sheet-section question-panel" aria-label="학습 모드와 질문 관리">
            <p className="question-panel__mode">
                <span>학습 모드</span>
                <strong>{modeSummary}</strong>
            </p>
            <div className="action-row">
                <button className="control-button secondary" onClick={onOpenLearningMode}>
                    학습 모드 관리
                </button>
                <button className="control-button secondary" onClick={onOpenQuestionGroupManager}>
                    질문 그룹/태그 관리
                </button>
                <button className="control-button secondary" onClick={onOpenGroupQuestionManager}>
                    그룹 내 질문 관리
                </button>
            </div>
        </section>
    )
}
