import React from 'react'

export function VoicePanel({
                               voiceConnected,
                               sessionActive,
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
                               handleAudioQuickAction
                           }) {
    let stageCaption = '세션 시작을 누르면 첫 질문이 자동으로 제시됩니다.'
    if (sessionActive) {
        if (speechState === 'READY_FOR_ANSWER') {
            stageCaption = '지금 답변해 주세요.'
        } else if (speechState === 'CAPTURING_ANSWER') {
            stageCaption = '답변을 듣고 있습니다.'
        } else if (speechState === 'PLAYING_FEEDBACK_TTS' || speechState === 'PLAYING_QUESTION_TTS') {
            stageCaption = '음성 출력 중입니다. 잠시만 기다려 주세요.'
        } else if (speechState === 'WAITING_TTS') {
            stageCaption = '다음 음성을 준비 중입니다.'
        } else if (speechState === 'STOPPING') {
            stageCaption = '세션을 정리하고 있습니다.'
        } else {
            stageCaption = '세션 진행 중입니다. 질문을 준비하고 있습니다.'
        }
    }

    return (
        <section className="panel voice-panel">
            <div className="panel-head voice-panel__head">
                <h2>실전 연습</h2>
                <span className={`state-chip ${voiceConnected ? 'is-on' : ''}`}>
                    {voiceConnected ? '음성 세션 연결됨' : '음성 세션 미연결'}
                </span>
            </div>

            <div className={`voice-stage voice-panel__stage ${voiceConnected ? 'is-connected' : ''}`}>
                <div className="voice-orb voice-panel__orb"/>
                <p className="stage-caption voice-panel__caption">
                    {stageCaption}
                </p>
                {partialTranscript &&
                    <div className="partial-transcript voice-panel__partial">{partialTranscript}</div>}
            </div>

            <div className="player-row voice-panel__actions">
                <button
                    className="action-button primary"
                    onClick={startSession}
                    disabled={sessionActive}
                >
                    세션 시작
                </button>
                <button
                    className="action-button danger"
                    onClick={stopSession}
                    disabled={!sessionActive}
                >
                    세션 종료
                </button>
            </div>

            <div className={`audio-quick-strip voice-panel__audio-strip ${audioConnectionReady ? 'is-ready' : ''}`}>
                <div className="audio-quick-copy voice-panel__audio-copy">
                    <strong>{audioConnectionLabel}</strong>
                    <span>권한 {audioPermissionLabel}</span>
                    <span>{audioQuickHint}</span>
                    {audioDeviceStatus.lastCheckedAt && <span>확인 {audioDeviceStatus.lastCheckedAt}</span>}
                </div>
                <button
                    className="audio-quick-button"
                    onClick={handleAudioQuickAction}
                    disabled={sessionActive || audioPermission === 'unsupported'}
                >
                    {audioPermission === 'granted' ? '상태 확인' : '권한 요청'}
                </button>
            </div>
        </section>
    )
}
