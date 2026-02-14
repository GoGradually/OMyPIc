import React from 'react'
import {MODE_OPTIONS} from '../../../shared/constants/models.js'

export function LearningModeOverlay({
                                        mode,
                                        setMode,
                                        batchSize,
                                        setBatchSize,
                                        updateMode,
                                        tagStats,
                                        selectedGroupTags,
                                        toggleSelectedTag
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
            </div>
        </div>
    )
}
