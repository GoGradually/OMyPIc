import React from 'react'

export function QuestionPanel({
                                  currentQuestionLabel,
                                  modeSummary,
                                  onOpenQuestionGroupManager,
                                  onOpenGroupQuestionManager,
                                  onOpenLearningMode
                              }) {
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
                <div className="action-row">
                    <button
                        className="control-button secondary"
                        onClick={onOpenLearningMode}
                    >
                        학습 모드 관리
                    </button>
                    <button
                        className="control-button secondary"
                        onClick={onOpenQuestionGroupManager}
                    >
                        질문 그룹/태그 관리
                    </button>
                    <button
                        className="control-button secondary"
                        onClick={onOpenGroupQuestionManager}
                    >
                        그룹 내 질문 관리
                    </button>
                </div>
            </div>
        </section>
    )
}
