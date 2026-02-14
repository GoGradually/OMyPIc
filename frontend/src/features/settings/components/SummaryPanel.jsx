import React from 'react'

export function SummaryPanel({
                                 feedbackLang,
                                 voice,
                                 voiceSttModel,
                                 feedbackModel,
                                 ttsModel,
                                 enabledRulebookCount,
                                 questionGroupCount,
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
                    <span>STT 모델</span>
                    <strong>{voiceSttModel}</strong>
                </li>
                <li>
                    <span>피드백 모델</span>
                    <strong>{feedbackModel}</strong>
                </li>
                <li>
                    <span>TTS 모델</span>
                    <strong>{ttsModel}</strong>
                </li>
                <li>
                    <span>활성 룰북</span>
                    <strong>{enabledRulebookCount}개</strong>
                </li>
                <li>
                    <span>질문 그룹</span>
                    <strong>{questionGroupCount}개</strong>
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
