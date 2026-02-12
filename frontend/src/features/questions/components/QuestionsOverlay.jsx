import React from 'react'
import {MODE_OPTIONS} from '../../../shared/constants/models.js'

export function QuestionsOverlay({
                                     mode,
                                     setMode,
                                     batchSize,
                                     setBatchSize,
                                     updateMode,
                                     nextQuestion,
                                     activeListId,
                                     newListName,
                                     setNewListName,
                                     questionLists,
                                     setActiveListId,
                                     createList,
                                     deleteList,
                                     newQuestion,
                                     setNewQuestion,
                                     newGroup,
                                     setNewGroup,
                                     addQuestion,
                                     activeQuestionList,
                                     editingQuestionId,
                                     editingQuestionText,
                                     setEditingQuestionText,
                                     editingQuestionGroup,
                                     setEditingQuestionGroup,
                                     startEditQuestion,
                                     saveEditedQuestion,
                                     cancelEditQuestion,
                                     removeQuestion
                                 }) {
    return (
        <div className="overlay-content">
            <div className="mode-tabs">
                {MODE_OPTIONS.map((option) => (
                    <button
                        key={option.value}
                        className={`mode-tab ${mode === option.value ? 'is-active' : ''}`}
                        onClick={() => setMode(option.value)}
                    >
                        {option.label}
                    </button>
                ))}
            </div>

            <div className="field-grid two-col">
                {mode === 'CONTINUOUS' && (
                    <div className="field-block">
                        <label>묶음 피드백 개수 (1~10)</label>
                        <input
                            type="number"
                            min="1"
                            max="10"
                            value={batchSize}
                            onChange={(event) => setBatchSize(Number(event.target.value))}
                        />
                    </div>
                )}
            </div>

            <div className="action-row">
                <button className="control-button" onClick={updateMode}>모드 적용</button>
                <button className="control-button secondary" onClick={nextQuestion} disabled={!activeListId}>
                    다음 질문
                </button>
            </div>

            <div className="field-grid two-col">
                <div className="field-block">
                    <label>새 리스트 이름</label>
                    <input
                        value={newListName}
                        onChange={(event) => setNewListName(event.target.value)}
                        placeholder="예: 자주 나오는 주제"
                    />
                </div>
                <div className="field-block">
                    <label>리스트 선택</label>
                    <select value={activeListId} onChange={(event) => setActiveListId(event.target.value)}>
                        {questionLists.length === 0 && <option value="">리스트 없음</option>}
                        {questionLists.map((list) => (
                            <option key={list.id} value={list.id}>{list.name}</option>
                        ))}
                    </select>
                </div>
            </div>

            <div className="action-row">
                <button className="control-button" onClick={createList}>리스트 생성</button>
                <button className="control-button secondary" onClick={deleteList} disabled={!activeListId}>
                    리스트 삭제
                </button>
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
                    <label>그룹</label>
                    <input
                        value={newGroup}
                        onChange={(event) => setNewGroup(event.target.value)}
                        placeholder="예: Group A"
                    />
                </div>
            </div>

            <button className="control-button" onClick={addQuestion} disabled={!activeListId}>질문 추가</button>

            {activeQuestionList && activeQuestionList.questions && activeQuestionList.questions.length > 0 && (
                <ul className="item-list question-items">
                    {activeQuestionList.questions.map((item) => (
                        <li key={item.id}>
                            {editingQuestionId === item.id ? (
                                <div className="edit-box">
                                    <input
                                        value={editingQuestionText}
                                        onChange={(event) => setEditingQuestionText(event.target.value)}
                                    />
                                    <input
                                        value={editingQuestionGroup}
                                        onChange={(event) => setEditingQuestionGroup(event.target.value)}
                                        placeholder="Group"
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
                                        {item.group && <small>{item.group}</small>}
                                    </div>
                                    <div className="item-actions">
                                        <button className="control-button secondary"
                                                onClick={() => startEditQuestion(item)}>
                                            수정
                                        </button>
                                        <button className="control-button secondary"
                                                onClick={() => removeQuestion(item.id)}>
                                            삭제
                                        </button>
                                    </div>
                                </>
                            )}
                        </li>
                    ))}
                </ul>
            )}

            {(!activeQuestionList || !activeQuestionList.questions || activeQuestionList.questions.length === 0) && (
                <p className="empty-text">선택된 리스트에 질문이 없습니다.</p>
            )}
        </div>
    )
}
