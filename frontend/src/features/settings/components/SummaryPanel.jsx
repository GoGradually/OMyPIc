import React from 'react'
import {DEVICE_TTS_MODEL, getModelLabel, LOCAL_TTS_MODEL} from '../../../shared/constants/models.js'

export function SummaryPanel({
                                 feedbackLang,
                                 voice,
                                 voiceSttModel,
                                 feedbackModel,
                                 aiEnabled = true,
                                 ttsModel,
                                 enabledRulebookCount,
                                 questionGroupCount,
                                 onOpenSettings
                             }) {
    const rows = [
        {label: 'AI 사용', value: aiEnabled ? '켜짐' : '꺼짐'},
        {label: '피드백 모델', value: feedbackModel, title: getModelLabel(feedbackModel), muted: !aiEnabled},
        {label: '피드백 언어', value: feedbackLang === 'ko' ? '한국어' : 'English', muted: !aiEnabled},
        {label: 'STT 모델', value: voiceSttModel, muted: !aiEnabled},
        // AI off still reads questions aloud: with the selected local model, or else the device voice.
        {label: 'TTS 모델', value: aiEnabled || ttsModel === LOCAL_TTS_MODEL ? ttsModel : DEVICE_TTS_MODEL},
        {label: '음성 스타일', value: voice, muted: !aiEnabled || ttsModel === DEVICE_TTS_MODEL || ttsModel === LOCAL_TTS_MODEL},
        {label: '활성 룰북', value: `${enabledRulebookCount}개`},
        {label: '질문 그룹', value: `${questionGroupCount}개`}
    ]

    return (
        <section className="side-section summary-panel">
            <div className="section-head">
                <h3>학습 설정 요약</h3>
                <button className="control-button secondary" onClick={onOpenSettings}>
                    설정 열기
                </button>
            </div>
            <dl className="summary-list">
                {rows.map((row) => (
                    <div key={row.label} className={row.muted ? 'is-muted' : ''}>
                        <dt>{row.label}</dt>
                        <dd title={row.title}>{row.value}</dd>
                    </div>
                ))}
            </dl>
        </section>
    )
}
