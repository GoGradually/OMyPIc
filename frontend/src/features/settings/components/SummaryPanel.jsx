import React from 'react'

export function SummaryPanel({
                                 feedbackLang,
                                 voice,
                                 realtimeConversationModel,
                                 realtimeSttModel,
                                 feedbackModel,
                                 enabledRulebookCount,
                                 questionListCount,
                                 onOpenSettings
                             }) {
    return (
        <section className="panel compact-panel summary-panel">
            <h3>학습 설정 요약</h3>
            <ul className="summary-list">
                <li>
                    <span>피드백 언어</span>
                    <strong>{feedbackLang === 'ko' ? '한국어' : 'English'}</strong>
                </li>
                <li>
                    <span>음성 스타일</span>
                    <strong>{voice}</strong>
                </li>
                <li>
                    <span>실시간 대화 모델</span>
                    <strong>{realtimeConversationModel}</strong>
                </li>
                <li>
                    <span>실시간 STT 모델</span>
                    <strong>{realtimeSttModel}</strong>
                </li>
                <li>
                    <span>실시간 피드백 모델</span>
                    <strong>{feedbackModel}</strong>
                </li>
                <li>
                    <span>활성 룰북</span>
                    <strong>{enabledRulebookCount}개</strong>
                </li>
                <li>
                    <span>질문 리스트</span>
                    <strong>{questionListCount}개</strong>
                </li>
            </ul>
            <div className="action-row">
                <button className="control-button secondary" onClick={onOpenSettings}>
                    설정 열기
                </button>
            </div>
        </section>
    )
}
