import React from 'react'
import {FEEDBACK_LANGUAGE_LABELS, getModelLabel} from '../../../shared/constants/models.js'

export function ModelSettingsOverlay({
                                         voiceSttModel,
                                         setVoiceSttModel,
                                         feedbackModel,
                                         setFeedbackModel,
                                         ttsModel,
                                         setTtsModel,
                                         feedbackModelOptions = [],
                                         voiceSttModelOptions = [],
                                         ttsModelOptions = [],
                                         apiKeyInput,
                                         setApiKeyInput,
                                         voice,
                                         setVoice,
                                         voiceOptions = [],
                                         feedbackLang,
                                         setFeedbackLang,
                                         feedbackLanguageOptions = [],
                                         onSaveApiKey
                                     }) {
    const feedbackLanguageList = Array.isArray(feedbackLanguageOptions) && feedbackLanguageOptions.length > 0
        ? feedbackLanguageOptions
        : ['ko', 'en']

    return (
        <div className="overlay-content">
            <div className="field-grid two-col">
                <div className="field-block">
                    <label>STT 모델</label>
                    <select value={voiceSttModel} onChange={(event) => setVoiceSttModel(event.target.value)}>
                        {voiceSttModelOptions.map((modelId) => (
                            <option key={modelId} value={modelId}>{getModelLabel(modelId)}</option>
                        ))}
                    </select>
                </div>
                <div className="field-block">
                    <label>피드백 모델</label>
                    <select value={feedbackModel} onChange={(event) => setFeedbackModel(event.target.value)}>
                        {feedbackModelOptions.map((modelId) => (
                            <option key={modelId} value={modelId}>{getModelLabel(modelId)}</option>
                        ))}
                    </select>
                </div>
                <div className="field-block">
                    <label>TTS 모델</label>
                    <select value={ttsModel} onChange={(event) => setTtsModel(event.target.value)}>
                        {ttsModelOptions.map((modelId) => (
                            <option key={modelId} value={modelId}>{getModelLabel(modelId)}</option>
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
                        {voiceOptions.map((item) => (
                            <option key={item} value={item}>{item}</option>
                        ))}
                    </select>
                </div>
                <div className="field-block">
                    <label>피드백 언어</label>
                    <select value={feedbackLang} onChange={(event) => setFeedbackLang(event.target.value)}>
                        {feedbackLanguageList.map((item) => (
                            <option key={item} value={item}>{FEEDBACK_LANGUAGE_LABELS[item] || item}</option>
                        ))}
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
