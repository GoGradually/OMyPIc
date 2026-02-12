import React from 'react'
import {MODE_OPTIONS} from '../../../shared/constants/models.js'

export function QuestionsOverlay({
                                     mode,
                                     setMode,
                                     batchSize,
                                     setBatchSize,
                                     updateMode,
                                     nextQuestion,
                                     tagStats,
                                     selectedGroupTags,
                                     toggleSelectedTag,
                                     activeGroupId,
                                     newGroupName,
                                     setNewGroupName,
                                     newGroupTagsInput,
                                     setNewGroupTagsInput,
                                     questionGroups,
                                     setActiveGroupId,
                                     createGroup,
                                     deleteGroup,
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
                        <label>묶음 피드백 그룹 수 (1~10)</label>
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

            <div className="field-block">
                <label>출제 태그 선택</label>
                <div className="mode-tabs">
                    {tagStats.length === 0 && (
                        <span className="tiny-meta">사용 가능한 태그가 없습니다.</span>
                    )}
                    {tagStats.map((tag) => (
                        <button
                            key={tag.tag}
                            className={`mode-tab ${selectedGroupTags.includes(tag.tag) ? 'is-active' : ''}`}
                            onClick={() => toggleSelectedTag(tag.tag, tag.selectable)}
                            disabled={!tag.selectable}
                        >
                            {tag.tag} ({tag.groupCount})
                        </button>
                    ))}
                </div>
            </div>

            <div className="action-row">
                <button className="control-button" onClick={updateMode}>모드 적용</button>
                <button className="control-button secondary" onClick={nextQuestion}>
                    다음 질문
                </button>
            </div>

            <div className="field-grid two-col">
                <div className="field-block">
                    <label>새 질문 그룹 이름</label>
                    <input
                        value={newGroupName}
                        onChange={(event) => setNewGroupName(event.target.value)}
                        placeholder="예: 여행/카페"
                    />
                </div>
                <div className="field-block">
                    <label>그룹 태그(쉼표 구분)</label>
                    <input
                        value={newGroupTagsInput}
                        onChange={(event) => setNewGroupTagsInput(event.target.value)}
                        placeholder="travel, habit"
                    />
                </div>
            </div>

            <div className="field-grid two-col">
                <div className="action-row">
                    <button className="control-button" onClick={createGroup}>그룹 생성</button>
                    <button className="control-button secondary" onClick={deleteGroup} disabled={!activeGroupId}>
                        그룹 삭제
                    </button>
                </div>
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

            {(!activeQuestionGroup || !activeQuestionGroup.questions || activeQuestionGroup.questions.length === 0) && (
                <p className="empty-text">선택된 질문 그룹에 질문이 없습니다.</p>
            )}
        </div>
    )
}
