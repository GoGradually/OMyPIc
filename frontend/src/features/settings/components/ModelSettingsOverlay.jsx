import React from 'react'
import {
    FEEDBACK_MODEL_OPTIONS,
    TTS_MODEL_OPTIONS,
    VOICE_STT_MODEL_OPTIONS,
    VOICES
} from '../../../shared/constants/models.js'

export function ModelSettingsOverlay({
                                         voiceSttModel,
                                         setVoiceSttModel,
                                         feedbackModel,
                                         setFeedbackModel,
                                         ttsModel,
                                         setTtsModel,
                                         apiKeyInput,
                                         setApiKeyInput,
                                         voice,
                                         setVoice,
                                         feedbackLang,
                                         setFeedbackLang,
                                         onSaveApiKey
                                     }) {
    return (
        <div className="overlay-content">
            <div className="field-grid two-col">
                <div className="field-block">
                    <label>STT 모델</label>
                    <select value={voiceSttModel} onChange={(event) => setVoiceSttModel(event.target.value)}>
                        {VOICE_STT_MODEL_OPTIONS.map((item) => (
                            <option key={item.value} value={item.value}>{item.label}</option>
                        ))}
                    </select>
                </div>
                <div className="field-block">
                    <label>피드백 모델</label>
                    <select value={feedbackModel} onChange={(event) => setFeedbackModel(event.target.value)}>
                        {FEEDBACK_MODEL_OPTIONS.map((item) => (
                            <option key={item.value} value={item.value}>{item.label}</option>
                        ))}
                    </select>
                </div>
                <div className="field-block">
                    <label>TTS 모델</label>
                    <select value={ttsModel} onChange={(event) => setTtsModel(event.target.value)}>
                        {TTS_MODEL_OPTIONS.map((item) => (
                            <option key={item.value} value={item.value}>{item.label}</option>
                        ))}
                    </select>
                </div>
            </div>

            <div className="field-block">
                <label>API Key</label>
                <input
                    value={apiKeyInput}
                    onChange={(event) => setApiKeyInput(event.target.value)}
                    placeholder="API Key 입력"
                />
            </div>

            <div className="field-grid two-col">
                <div className="field-block">
                    <label>음성 스타일</label>
                    <select value={voice} onChange={(event) => setVoice(event.target.value)}>
                        {VOICES.map((item) => (
                            <option key={item} value={item}>{item}</option>
                        ))}
                    </select>
                </div>
                <div className="field-block">
                    <label>피드백 언어</label>
                    <select value={feedbackLang} onChange={(event) => setFeedbackLang(event.target.value)}>
                        <option value="ko">한국어</option>
                        <option value="en">English</option>
                    </select>
                </div>
            </div>

            <div className="action-row">
                <button className="control-button" onClick={onSaveApiKey}>저장 + 검증</button>
            </div>

            <p className="tiny-meta">음성 세션 기능은 OpenAI API Key가 필요합니다.</p>
            <p className="tiny-meta">저장 + 검증은 API Key 권한 확인이며, 모델 유효성은 별도로 확인됩니다.</p>
            <p className="tiny-meta">기본 설정(gpt-5-nano) 기준으로 세션 1회당 약 50원이 사용됩니다.</p>
            <p className="tiny-meta">실제 비용은 발화 길이와 선택 모델에 따라 달라질 수 있습니다.</p>
        </div>
    )
}
