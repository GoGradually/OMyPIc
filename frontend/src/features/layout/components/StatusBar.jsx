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
        <footer className="status">
            <div className="status-bar">
                <p className="status-bar__message" role="status">{statusMessage || '준비 완료'}</p>
                <span className={`status-bar__item ${voiceConnected ? 'is-on' : ''}`}>
                    {voiceConnected ? '음성 세션 연결됨' : '음성 세션 대기'}
                </span>
                <span className="status-bar__item">{audioConnectionLabel}</span>
                <button
                    className="status-toggle"
                    aria-expanded={showStatusDetails}
                    onClick={onToggleStatusDetails}
                >
                    {showStatusDetails ? '상세 숨기기' : '상세 보기'}
                </button>
            </div>

            {showStatusDetails && (
                <dl className="status-details">
                    <div>
                        <dt>세션</dt>
                        <dd>{statusDetails.sessionIdPrefix}</dd>
                    </div>
                    <div>
                        <dt>마이크 권한</dt>
                        <dd>{statusDetails.audioPermissionLabel}</dd>
                    </div>
                    <div>
                        <dt>입력 장치</dt>
                        <dd>{statusDetails.audioInputCount}개</dd>
                    </div>
                    <div>
                        <dt>STT 모델</dt>
                        <dd>{statusDetails.voiceSttModel}</dd>
                    </div>
                    <div>
                        <dt>피드백 모델</dt>
                        <dd>{statusDetails.feedbackModel}</dd>
                    </div>
                    <div>
                        <dt>TTS 모델</dt>
                        <dd>{statusDetails.ttsModel}</dd>
                    </div>
                </dl>
            )}
        </footer>
    )
}
