import React from 'react'

export function VoicePanel({
                               realtimeConnected,
                               recording,
                               partialTranscript,
                               startRecording,
                               stopRecording,
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
                    {recording ? '음성을 수집하는 중입니다.' : '녹음 시작을 누르고 답변을 말해보세요.'}
                </p>
                {partialTranscript &&
                    <div className="partial-transcript realtime-panel__partial">{partialTranscript}</div>}
            </div>

            <div className="player-row realtime-panel__actions">
                <button
                    className="action-button primary"
                    onClick={startRecording}
                    disabled={recording}
                >
                    녹음 시작
                </button>
                <button
                    className="action-button danger"
                    onClick={stopRecording}
                    disabled={!recording}
                >
                    녹음 중지
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
                    disabled={recording || audioPermission === 'unsupported'}
                >
                    {audioPermission === 'granted' ? '상태 확인' : '권한 요청'}
                </button>
            </div>
        </section>
    )
}
