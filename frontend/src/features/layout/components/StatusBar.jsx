import React from 'react'

export function StatusBar({
                              voiceConnected,
                              audioConnectionLabel,
                              statusMessage,
                              showStatusDetails,
                              onToggleStatusDetails,
                              statusDetails
                          }) {
    return (
        <>
            <div className="status-bar app__status-bar">
                <div className="status-main app__status-main">
                    <span className={`status-dot ${voiceConnected ? 'is-on' : ''}`}/>
                    <span>{voiceConnected ? '음성 세션 연결됨' : '음성 세션 미연결'}</span>
                    <span className="status-divider">·</span>
                    <span>{audioConnectionLabel}</span>
                    <span className="status-divider">·</span>
                    <span>{statusMessage || '준비 완료'}</span>
                </div>
                <button className="status-toggle" onClick={onToggleStatusDetails}>
                    {showStatusDetails ? '상세 숨기기' : '상세 보기'}
                </button>
            </div>

            {showStatusDetails && (
                <div className="status-details app__status-details">
                    <span>세션 {statusDetails.sessionIdPrefix}</span>
                    <span>마이크 권한 {statusDetails.audioPermissionLabel}</span>
                    <span>입력 장치 {statusDetails.audioInputCount}개</span>
                    <span>STT 모델 {statusDetails.voiceSttModel}</span>
                    <span>피드백 모델 {statusDetails.feedbackModel}</span>
                </div>
            )}
        </>
    )
}
