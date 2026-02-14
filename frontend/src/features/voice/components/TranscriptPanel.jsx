import React from 'react'

export function TranscriptPanel({userText, transcript, onCopyUserText}) {
    return (
        <section className="panel transcript-panel">
            <div className="panel-head transcript-panel__head">
                <h3>인식 텍스트</h3>
                <button
                    className="control-button secondary"
                    onClick={onCopyUserText}
                    disabled={!userText.trim()}
                >
                    복사
                </button>
            </div>

            <textarea
                value={userText}
                readOnly
                placeholder="녹음이 끝나면 인식된 문장이 여기에 표시됩니다."
            />
            {transcript && <p className="tiny-meta">최근 확정 인식: {transcript}</p>}
        </section>
    )
}
