import React from 'react'

export function QuestionGroupManagerOverlay({
                                                activeGroupId,
                                                newGroupName,
                                                setNewGroupName,
                                                newGroupTagsInput,
                                                setNewGroupTagsInput,
                                                questionGroups,
                                                setActiveGroupId,
                                                createGroup,
                                                deleteGroup
                                            }) {
    return (
        <div className="overlay-content">
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
        </div>
    )
}
