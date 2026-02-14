import React from 'react'

export function GroupQuestionManagerOverlay({
                                                activeGroupId,
                                                questionGroups,
                                                setActiveGroupId,
                                                newQuestion,
                                                setNewQuestion,
                                                newQuestionType,
                                                setNewQuestionType,
                                                addQuestion,
                                                activeQuestionGroup,
                                                editingQuestionId,
                                                editingQuestionText,
                                                setEditingQuestionText,
                                                editingQuestionType,
                                                setEditingQuestionType,
                                                startEditQuestion,
                                                saveEditedQuestion,
                                                cancelEditQuestion,
                                                removeQuestion
                                            }) {
    return (
        <div className="overlay-content">
            <div className="field-block">
                <label>질문 그룹 선택</label>
                <select value={activeGroupId} onChange={(event) => setActiveGroupId(event.target.value)}>
                    {questionGroups.length === 0 && <option value="">그룹 없음</option>}
                    {questionGroups.map((group) => (
                        <option key={group.id} value={group.id}>
                            {group.name} [{(group.tags || []).join(', ')}]
                        </option>
                    ))}
                </select>
            </div>

            <div className="field-grid two-col">
                <div className="field-block">
                    <label>질문 추가</label>
                    <input
                        value={newQuestion}
                        onChange={(event) => setNewQuestion(event.target.value)}
                        placeholder="연습할 질문 입력"
                    />
                </div>
                <div className="field-block">
                    <label>질문 타입(선택)</label>
                    <input
                        value={newQuestionType}
                        onChange={(event) => setNewQuestionType(event.target.value)}
                        placeholder="예: habit, compare"
                    />
                </div>
            </div>

            <button className="control-button" onClick={addQuestion} disabled={!activeGroupId}>질문 추가</button>

            {activeQuestionGroup && activeQuestionGroup.questions && activeQuestionGroup.questions.length > 0 && (
                <ul className="item-list question-items">
                    {activeQuestionGroup.questions.map((item) => (
                        <li key={item.id}>
                            {editingQuestionId === item.id ? (
                                <div className="edit-box">
                                    <input
                                        value={editingQuestionText}
                                        onChange={(event) => setEditingQuestionText(event.target.value)}
                                    />
                                    <input
                                        value={editingQuestionType}
                                        onChange={(event) => setEditingQuestionType(event.target.value)}
                                        placeholder="questionType"
                                    />
                                    <div className="item-actions">
                                        <button className="control-button" onClick={saveEditedQuestion}>저장</button>
                                        <button className="control-button secondary" onClick={cancelEditQuestion}>취소
                                        </button>
                                    </div>
                                </div>
                            ) : (
                                <>
                                    <div className="question-line">
                                        <span>{item.text}</span>
                                        {item.questionType && <small>{item.questionType}</small>}
                                    </div>
                                    <div className="item-actions">
                                        <button
                                            className="control-button secondary"
                                            onClick={() => startEditQuestion(item)}
                                        >
                                            수정
                                        </button>
                                        <button
                                            className="control-button secondary"
                                            onClick={() => removeQuestion(item.id)}
                                        >
                                            삭제
                                        </button>
                                    </div>
                                </>
                            )}
                        </li>
                    ))}
                </ul>
            )}

            {(!activeQuestionGroup || !activeQuestionGroup.questions || activeQuestionGroup.questions.length === 0) && (
                <p className="empty-text">선택된 질문 그룹에 질문이 없습니다.</p>
            )}
        </div>
    )
}
