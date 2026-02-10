import React from 'react'

export function VoicePanel({
                               realtimeConnected,
                               sessionActive,
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
    return (
        <section className="panel voice-panel realtime-panel">
            <div className="panel-head realtime-panel__head">
                <h2>실전 연습</h2>
                <span className={`state-chip ${realtimeConnected ? 'is-on' : ''}`}>
                    {realtimeConnected ? '실시간 연결됨' : '실시간 미연결'}
                </span>
            </div>

            <div className={`voice-stage realtime-panel__stage ${realtimeConnected ? 'is-connected' : ''}`}>
                <div className="voice-orb realtime-panel__orb"/>
                <p className="stage-caption realtime-panel__caption">
                    {sessionActive ? '세션 진행 중입니다. 질문에 답변해 주세요.' : '세션 시작을 누르면 첫 질문이 자동으로 제시됩니다.'}
                </p>
                {partialTranscript &&
                    <div className="partial-transcript realtime-panel__partial">{partialTranscript}</div>}
            </div>

            <div className="player-row realtime-panel__actions">
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

            <div className={`audio-quick-strip realtime-panel__audio-strip ${audioConnectionReady ? 'is-ready' : ''}`}>
                <div className="audio-quick-copy realtime-panel__audio-copy">
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
