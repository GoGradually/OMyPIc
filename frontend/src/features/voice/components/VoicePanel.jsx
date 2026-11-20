import React from 'react'
import {getActiveStepIndex, getPracticeSteps, getStageCaption} from '../utils/practiceStage.js'

function stepState(index, activeIndex) {
    if (activeIndex < 0) {
        return 'idle'
    }
    if (index === activeIndex) {
        return 'current'
    }
    return index < activeIndex ? 'done' : 'next'
}

export function VoicePanel({
                               voiceConnected,
                               sessionActive,
                               sessionStarting = false,
                               speechState,
                               partialTranscript,
                               startSession,
                               stopSession,
                               audioConnectionReady,
                               audioConnectionLabel,
                               audioPermissionLabel,
                               audioQuickHint,
                               audioPermission,
                               audioDeviceStatus,
                               handleAudioQuickAction,
                               replayButtonLabel,
                               replayButtonDisabled,
                               replayButtonDisabledReason,
                               handleReplayAction,
                               replayPlaying = false,
                               awaitingReview = false,
                               currentQuestionLabel = '',
                               hasQuestion = false,
                               stageAiEnabled = true,
                               aiEnabled = true,
                               onToggleAi,
                               autoReplayEnabled = false,
                               onToggleAutoReplay,
                               feedbackSummary = '',
                               continuousMode = false,
                               speechModelNote = ''
                           }) {
    const stageOptions = {aiEnabled: stageAiEnabled, autoReplayEnabled, continuousMode}
    const steps = getPracticeSteps(stageOptions)
    const activeStep = getActiveStepIndex({
        sessionActive,
        speechState,
        replayPlaying,
        awaitingReview,
        stepCount: steps.length
    })
    const caption = getStageCaption({sessionActive, speechState, replayPlaying, awaitingReview, ...stageOptions})
    const recording = sessionActive && speechState === 'CAPTURING_ANSWER'

    return (
        <section className="sheet-section voice-panel" aria-label="실전 연습">
            <div className="voice-panel__head">
                <h2>실전 연습</h2>
                <span className={`connection ${voiceConnected ? 'is-on' : ''}`}>
                    {voiceConnected ? '음성 세션 연결됨' : '음성 세션 대기'}
                </span>
            </div>

            <ol className="steps" aria-label="연습 순서">
                {steps.map((label, index) => {
                    const state = stepState(index, activeStep)
                    return (
                        <li
                            key={label}
                            className={`step is-${state}`}
                            aria-current={state === 'current' ? 'step' : undefined}
                        >
                            <span className="step__number">{index + 1}</span>
                            <span className="step__label">{label}</span>
                        </li>
                    )
                })}
            </ol>

            <p className={`question ${hasQuestion ? '' : 'is-placeholder'}`} lang={hasQuestion ? 'en' : undefined}>
                {currentQuestionLabel}
            </p>

            <p className="caption" role="status">
                {recording && <span className="rec-dot" aria-hidden="true"/>}
                {caption}
            </p>
            {speechModelNote && <p className="voice-note" role="note">{speechModelNote}</p>}
            {partialTranscript && <p className="partial-transcript">{partialTranscript}</p>}

            <div className="voice-panel__actions">
                <div className="button-group">
                    <button className="action-button primary" onClick={startSession} disabled={sessionActive || sessionStarting}>
                        세션 시작
                    </button>
                    <button className="action-button danger" onClick={stopSession} disabled={!sessionActive && !sessionStarting}>
                        세션 종료
                    </button>
                </div>
                <span
                    className="voice-panel__replay"
                    title={replayButtonDisabled && replayButtonDisabledReason ? replayButtonDisabledReason : undefined}
                >
                    <button
                        className="action-button neutral"
                        onClick={handleReplayAction}
                        disabled={replayButtonDisabled}
                    >
                        {replayButtonLabel}
                    </button>
                </span>
            </div>

            <div className="ai-control">
                <button
                    type="button"
                    role="switch"
                    className="switch"
                    aria-checked={aiEnabled}
                    aria-label="AI 사용"
                    onClick={() => onToggleAi?.(!aiEnabled)}
                    disabled={sessionActive}
                >
                    <span className="switch__thumb"/>
                </button>
                <div className="ai-control__copy">
                    <strong>AI {aiEnabled ? '켜짐' : '꺼짐'}</strong>
                    <span>
                        {sessionActive
                            ? '세션 중에는 바꿀 수 없습니다. 다음 세션부터 적용됩니다.'
                            : aiEnabled
                                ? `질문을 읽어 주고, 답변을 ${feedbackSummary || '선택한 모델'}로 첨삭합니다.`
                                : 'API를 호출하지 않습니다. 질문은 로컬 Kokoro로 이 기기에서 합성해 읽고 답변은 녹음만 합니다. API Key 없이 진행합니다.'}
                    </span>
                </div>
                {!aiEnabled && (
                    <label className="check">
                        <input
                            type="checkbox"
                            checked={autoReplayEnabled}
                            onChange={(event) => onToggleAutoReplay?.(event.target.checked)}
                        />
                        답변 후 내 응답 자동 재생
                    </label>
                )}
            </div>

            <div className={`mic-strip ${audioConnectionReady ? 'is-ready' : ''}`}>
                <div className="mic-strip__copy">
                    <strong>{audioConnectionLabel}</strong>
                    <span>권한 {audioPermissionLabel}</span>
                    <span>{audioQuickHint}</span>
                    {audioDeviceStatus.lastCheckedAt && <span>확인 {audioDeviceStatus.lastCheckedAt}</span>}
                </div>
                <button
                    className="control-button secondary"
                    onClick={handleAudioQuickAction}
                    disabled={sessionActive || audioPermission === 'unsupported'}
                >
                    {audioPermission === 'granted' ? '상태 확인' : '권한 요청'}
                </button>
            </div>
        </section>
    )
}
