import React, {useState} from 'react'
import {
    FEEDBACK_LANGUAGE_LABELS,
    getModelLabel,
    getProviderLabel
} from '../../../shared/constants/models.js'

function ModelOptions({models}) {
    return models.map((modelId) => (
        <option key={modelId} value={modelId}>{getModelLabel(modelId)}</option>
    ))
}

function ApiKeyField({id, label, hint, value, onChange, placeholder}) {
    const [visible, setVisible] = useState(false)
    return (
        <div className="field-block">
            <label htmlFor={id}>{label}</label>
            <div className="key-field">
                <input
                    id={id}
                    type={visible ? 'text' : 'password'}
                    value={value}
                    onChange={(event) => onChange(event.target.value)}
                    placeholder={placeholder}
                    autoComplete="off"
                    spellCheck={false}
                />
                <button
                    type="button"
                    className="control-button secondary"
                    aria-pressed={visible}
                    onClick={() => setVisible((prev) => !prev)}
                >
                    {visible ? '숨기기' : '표시'}
                </button>
            </div>
            <p className="tiny-meta">{hint}</p>
        </div>
    )
}

export function ModelSettingsOverlay({
                                         voiceSttModel,
                                         setVoiceSttModel,
                                         feedbackModel,
                                         setFeedbackModel,
                                         ttsModel,
                                         setTtsModel,
                                         feedbackModelOptions = [],
                                         feedbackProviderGroups = [],
                                         feedbackProvider = 'openai',
                                         voiceSttModelOptions = [],
                                         ttsModelOptions = [],
                                         apiKeyInput,
                                         setApiKeyInput,
                                         claudeKeyInput = '',
                                         setClaudeKeyInput = () => {
                                         },
                                         aiFeedbackEnabled = true,
                                         setAiFeedbackEnabled = () => {
                                         },
                                         autoReplayEnabled = false,
                                         setAutoReplayEnabled = () => {
                                         },
                                         sessionActive = false,
                                         voice,
                                         setVoice,
                                         voiceOptions = [],
                                         feedbackLang,
                                         setFeedbackLang,
                                         feedbackLanguageOptions = [],
                                         onSaveApiKey,
                                         onExportData,
                                         onImportData
                                     }) {
    const feedbackLanguageList = Array.isArray(feedbackLanguageOptions) && feedbackLanguageOptions.length > 0
        ? feedbackLanguageOptions
        : ['ko', 'en']
    const usesClaude = feedbackProvider === 'anthropic'

    return (
        <div className="overlay-content settings">
            <section className="settings__group" aria-labelledby="settings-feedback">
                <h3 id="settings-feedback">AI 피드백</h3>
                <div className="ai-control">
                    <button
                        type="button"
                        role="switch"
                        className="switch"
                        aria-checked={aiFeedbackEnabled}
                        aria-label="AI 피드백 사용"
                        onClick={() => setAiFeedbackEnabled(!aiFeedbackEnabled)}
                        disabled={sessionActive}
                    >
                        <span className="switch__thumb"/>
                    </button>
                    <div className="ai-control__copy">
                        <strong>AI 피드백 {aiFeedbackEnabled ? '켜짐' : '꺼짐'}</strong>
                        <span>
                            {aiFeedbackEnabled
                                ? '답변마다 선택한 모델이 첨삭합니다.'
                                : '피드백 모델을 호출하지 않습니다. 질문 듣기와 답변 인식은 그대로 진행됩니다.'}
                        </span>
                    </div>
                    {!aiFeedbackEnabled && (
                        <label className="check">
                            <input
                                type="checkbox"
                                checked={autoReplayEnabled}
                                onChange={(event) => setAutoReplayEnabled(event.target.checked)}
                            />
                            답변 후 내 응답 자동 재생
                        </label>
                    )}
                </div>

                <div className="field-grid two-col">
                    <div className="field-block">
                        <label htmlFor="feedback-model">피드백 모델</label>
                        <select
                            id="feedback-model"
                            value={feedbackModel}
                            onChange={(event) => setFeedbackModel(event.target.value)}
                            disabled={!aiFeedbackEnabled}
                        >
                            {feedbackProviderGroups.length > 0
                                ? feedbackProviderGroups.map((group) => (
                                    <optgroup key={group.provider} label={getProviderLabel(group.provider)}>
                                        <ModelOptions models={group.models}/>
                                    </optgroup>
                                ))
                                : <ModelOptions models={feedbackModelOptions}/>}
                        </select>
                        {aiFeedbackEnabled && usesClaude && (
                            <p className="tiny-meta">Claude 모델은 아래 Claude API Key로 호출합니다.</p>
                        )}
                    </div>
                    <div className="field-block">
                        <label htmlFor="feedback-language">피드백 언어</label>
                        <select
                            id="feedback-language"
                            value={feedbackLang}
                            onChange={(event) => setFeedbackLang(event.target.value)}
                            disabled={!aiFeedbackEnabled}
                        >
                            {feedbackLanguageList.map((item) => (
                                <option key={item} value={item}>{FEEDBACK_LANGUAGE_LABELS[item] || item}</option>
                            ))}
                        </select>
                    </div>
                </div>
            </section>

            <section className="settings__group" aria-labelledby="settings-voice">
                <h3 id="settings-voice">질문 음성과 답변 인식</h3>
                <div className="field-grid three-col">
                    <div className="field-block">
                        <label htmlFor="stt-model">STT 모델</label>
                        <select
                            id="stt-model"
                            value={voiceSttModel}
                            onChange={(event) => setVoiceSttModel(event.target.value)}
                        >
                            <ModelOptions models={voiceSttModelOptions}/>
                        </select>
                    </div>
                    <div className="field-block">
                        <label htmlFor="tts-model">TTS 모델</label>
                        <select id="tts-model" value={ttsModel} onChange={(event) => setTtsModel(event.target.value)}>
                            <ModelOptions models={ttsModelOptions}/>
                        </select>
                    </div>
                    <div className="field-block">
                        <label htmlFor="tts-voice">음성 스타일</label>
                        <select id="tts-voice" value={voice} onChange={(event) => setVoice(event.target.value)}>
                            {voiceOptions.map((item) => (
                                <option key={item} value={item}>{item}</option>
                            ))}
                        </select>
                    </div>
                </div>
            </section>

            <section className="settings__group" aria-labelledby="settings-keys">
                <h3 id="settings-keys">API Key</h3>
                <ApiKeyField
                    id="openai-key"
                    label="OpenAI API Key"
                    hint="질문 음성(TTS)과 답변 인식(STT)에 항상 사용합니다. GPT 피드백 모델도 이 키로 호출합니다."
                    value={apiKeyInput}
                    onChange={setApiKeyInput}
                    placeholder="sk-..."
                />
                <ApiKeyField
                    id="claude-key"
                    label="Claude API Key"
                    hint="Claude 피드백 모델을 선택했을 때만 필요합니다. 비워 두면 저장된 키를 지웁니다."
                    value={claudeKeyInput}
                    onChange={setClaudeKeyInput}
                    placeholder="sk-ant-..."
                />
                <div className="action-row">
                    <button className="control-button" onClick={onSaveApiKey}>저장 + 검증</button>
                </div>
                <p className="tiny-meta">키는 이 기기의 OS 키체인에만 저장됩니다. 저장 + 검증은 키 권한을 확인합니다.</p>
            </section>

            <section className="settings__group" aria-labelledby="settings-cost">
                <h3 id="settings-cost">비용 안내</h3>
                <p className="tiny-meta">기본 설정(gpt-5-nano) 기준으로 세션 1회당 약 50원이 사용됩니다.</p>
                <p className="tiny-meta">실제 비용은 발화 길이와 선택 모델에 따라 달라질 수 있습니다.</p>
                <p className="tiny-meta">AI 피드백을 끄면 피드백 모델 비용이 들지 않습니다. 음성 합성과 인식 비용은 그대로입니다.</p>
            </section>

            <section className="settings__group" aria-labelledby="settings-data">
                <h3 id="settings-data">데이터</h3>
                <div className="action-row">
                    <button className="control-button secondary" onClick={onExportData}>데이터 내보내기</button>
                    <button className="control-button secondary" onClick={onImportData}>데이터 가져오기</button>
                </div>
            </section>
        </div>
    )
}
