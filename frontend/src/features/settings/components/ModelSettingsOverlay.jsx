import React from 'react'
import {
    FEEDBACK_MODELS,
    MOCK_FINAL_MODELS,
    REALTIME_CONVERSATION_MODELS,
    REALTIME_STT_MODELS,
    VOICES
} from '../../../shared/constants/models.js'

export function ModelSettingsOverlay({
                                         provider,
                                         setProvider,
                                         realtimeConversationModel,
                                         setRealtimeConversationModel,
                                         realtimeSttModel,
                                         setRealtimeSttModel,
                                         feedbackModel,
                                         setFeedbackModel,
                                         mockFinalModel,
                                         setMockFinalModel,
                                         apiKeyInput,
                                         setApiKeyInput,
                                         voice,
                                         setVoice,
                                         feedbackLang,
                                         setFeedbackLang,
                                         onSaveApiKey,
                                         onSyncRealtimeSettings
                                     }) {
    return (
        <div className="overlay-content">
            <div className="field-grid two-col">
                <div className="field-block">
                    <label>공급자</label>
                    <select value={provider} onChange={(event) => setProvider(event.target.value)}>
                        <option value="openai">OpenAI</option>
                        <option value="anthropic">Claude</option>
                        <option value="gemini">Gemini</option>
                    </select>
                </div>
                <div className="field-block">
                    <label>실시간 대화 모델</label>
                    <select value={realtimeConversationModel}
                            onChange={(event) => setRealtimeConversationModel(event.target.value)}>
                        {REALTIME_CONVERSATION_MODELS.map((item) => (
                            <option key={item} value={item}>{item}</option>
                        ))}
                    </select>
                </div>
                <div className="field-block">
                    <label>실시간 STT 모델</label>
                    <select value={realtimeSttModel} onChange={(event) => setRealtimeSttModel(event.target.value)}>
                        {REALTIME_STT_MODELS.map((item) => (
                            <option key={item} value={item}>{item}</option>
                        ))}
                    </select>
                </div>
                <div className="field-block">
                    <label>실시간 피드백 모델</label>
                    <select value={feedbackModel} onChange={(event) => setFeedbackModel(event.target.value)}>
                        {FEEDBACK_MODELS[provider].map((item) => (
                            <option key={item} value={item}>{item}</option>
                        ))}
                    </select>
                </div>
                <div className="field-block">
                    <label>모의고사 최종 피드백 모델</label>
                    <select value={mockFinalModel} onChange={(event) => setMockFinalModel(event.target.value)}>
                        {MOCK_FINAL_MODELS[provider].map((item) => (
                            <option key={item} value={item}>{item}</option>
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
                <button className="control-button secondary" onClick={onSyncRealtimeSettings}>실시간 설정 반영</button>
            </div>

            <p className="tiny-meta">실시간 음성 기능은 OpenAI API Key가 필요합니다.</p>
            <p className="tiny-meta">저장 + 검증은 API Key 권한 확인이며, 모델 유효성은 별도로 확인됩니다.</p>
        </div>
    )
}
