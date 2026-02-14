import React from 'react'

export function QuestionGroupManagerOverlay({
                                                activeGroupId,
                                                questionGroups,
                                                setActiveGroupId,
                                                isCreateGroupModalOpen,
                                                openCreateGroupModal,
                                                closeCreateGroupModal,
                                                newGroupName,
                                                setNewGroupName,
                                                newGroupTagsInput,
                                                setNewGroupTagsInput,
                                                createGroup,
                                                isEditGroupModalOpen,
                                                openEditGroupModal,
                                                closeEditGroupModal,
                                                editingGroupName,
                                                setEditingGroupName,
                                                editingGroupTagsInput,
                                                setEditingGroupTagsInput,
                                                deleteGroup,
                                                saveEditedGroup
                                            }) {
    return (
        <div className="overlay-content question-group-list-panel">
            <div className="action-row">
                <button className="control-button" onClick={openCreateGroupModal}>그룹 생성</button>
            </div>

            <section className="crud-section">
                <h3>그룹 목록</h3>
                {questionGroups.length === 0 && (
                    <p className="empty-text">등록된 질문 그룹이 없습니다.</p>
                )}
                {questionGroups.length > 0 && (
                    <ul className="item-list question-group-list-items">
                        {questionGroups.map((group) => (
                            <li key={group.id} className={group.id === activeGroupId ? 'is-active' : ''}>
                                <button
                                    className="group-row-select"
                                    onClick={() => setActiveGroupId(group.id)}
                                >
                                    <strong>{group.name}</strong>
                                    <small>{(group.tags || []).join(', ') || '태그 없음'}</small>
                                </button>
                                <div className="item-actions">
                                    <button
                                        className="control-button secondary"
                                        onClick={() => openEditGroupModal(group)}
                                    >
                                        수정
                                    </button>
                                    <button
                                        className="control-button secondary"
                                        onClick={() => deleteGroup(group.id)}
                                    >
                                        삭제
                                    </button>
                                </div>
                            </li>
                        ))}
                    </ul>
                )}
            </section>

            {isCreateGroupModalOpen && (
                <div className="inline-modal-backdrop" onClick={closeCreateGroupModal}>
                    <div
                        className="inline-modal"
                        role="dialog"
                        aria-modal="true"
                        aria-label="질문 그룹 생성"
                        onClick={(event) => event.stopPropagation()}
                    >
                        <h3>질문 그룹 생성</h3>
                        <div className="field-grid">
                            <div className="field-block">
                                <label>그룹 이름</label>
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
                        <div className="inline-modal-actions">
                            <button className="control-button secondary" onClick={closeCreateGroupModal}>취소</button>
                            <button className="control-button" onClick={createGroup}>생성</button>
                        </div>
                    </div>
                </div>
            )}

            {isEditGroupModalOpen && (
                <div className="inline-modal-backdrop" onClick={closeEditGroupModal}>
                    <div
                        className="inline-modal"
                        role="dialog"
                        aria-modal="true"
                        aria-label="질문 그룹 수정"
                        onClick={(event) => event.stopPropagation()}
                    >
                        <h3>질문 그룹 수정</h3>
                        <div className="field-grid">
                            <div className="field-block">
                                <label>그룹 이름</label>
                                <input
                                    value={editingGroupName}
                                    onChange={(event) => setEditingGroupName(event.target.value)}
                                    placeholder="예: 여행/카페"
                                />
                            </div>
                            <div className="field-block">
                                <label>그룹 태그(쉼표 구분)</label>
                                <input
                                    value={editingGroupTagsInput}
                                    onChange={(event) => setEditingGroupTagsInput(event.target.value)}
                                    placeholder="travel, habit"
                                />
                            </div>
                        </div>
                        <div className="inline-modal-actions">
                            <button className="control-button secondary" onClick={closeEditGroupModal}>취소</button>
                            <button className="control-button" onClick={saveEditedGroup}>저장</button>
                        </div>
                    </div>
                </div>
            )}
        </div>
    )
}
