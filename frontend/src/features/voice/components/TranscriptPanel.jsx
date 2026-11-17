import React from 'react'

export function TranscriptPanel({userText, transcript, onCopyUserText}) {
    return (
        <section className="sheet-section transcript-panel">
            <div className="section-head">
                <h3 id="transcript-title">인식 텍스트</h3>
                <button
                    className="control-button secondary"
                    onClick={onCopyUserText}
                    disabled={!userText.trim()}
                >
                    복사
                </button>
            </div>

            <textarea
                aria-labelledby="transcript-title"
                value={userText}
                readOnly
                lang="en"
                placeholder="답변을 마치면 인식된 문장이 여기에 표시됩니다."
            />
            {transcript && <p className="tiny-meta">최근 확정 인식: {transcript}</p>}
        </section>
    )
}
