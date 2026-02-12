import React from 'react'

export function RulebookOverlay({rulebooks, uploadRulebook, toggleRulebook, deleteRulebook}) {
    const [scope, setScope] = React.useState('MAIN')
    const [questionGroup, setQuestionGroup] = React.useState('')

    const canUploadQuestionScope = scope !== 'QUESTION' || Boolean(questionGroup.trim())

    return (
        <div className="overlay-content">
            <div className="field-block">
                <label>룰북 업로드 (.md)</label>
                <div className="field-grid two-col">
                    <select value={scope} onChange={(event) => setScope(event.target.value)}>
                        <option value="MAIN">메인 룰북</option>
                        <option value="QUESTION">질문별 룰북</option>
                    </select>
                    {scope === 'QUESTION' && (
                        <input
                            value={questionGroup}
                            onChange={(event) => setQuestionGroup(event.target.value)}
                            placeholder="질문 그룹 (예: A)"
                        />
                    )}
                </div>
                <input
                    type="file"
                    accept=".md"
                    disabled={!canUploadQuestionScope}
                    onChange={(event) => {
                        if (event.target.files[0]) {
                            uploadRulebook(event.target.files[0], scope, questionGroup.trim())
                            event.target.value = ''
                        }
                    }}
                />
                <p className="tiny-meta">마크다운 문서를 업로드하면 학습 참고 자료로 사용됩니다.</p>
            </div>

            <ul className="item-list">
                {rulebooks.map((book) => (
                    <li key={book.id}>
                        <span>
                            {book.filename}
                            <small> · {book.scope === 'QUESTION' ? `질문별(${book.questionGroup || '-'})` : '메인'}</small>
                        </span>
                        <div className="item-actions">
                            <button
                                className="control-button secondary"
                                onClick={() => toggleRulebook(book.id, !book.enabled)}
                            >
                                {book.enabled ? '사용 중지' : '사용'}
                            </button>
                            <button className="control-button secondary" onClick={() => deleteRulebook(book.id)}>
                                삭제
                            </button>
                        </div>
                    </li>
                ))}
            </ul>

            {rulebooks.length === 0 && <p className="empty-text">등록된 룰북이 없습니다.</p>}
        </div>
    )
}
