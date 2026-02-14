import React from 'react'

const QUICK_PANELS = [
    {key: 'rulebook', label: '룰북'},
    {key: 'question-group-manager', label: '그룹/태그 관리'},
    {key: 'group-question-manager', label: '그룹 내 질문 관리'},
    {key: 'learning-mode', label: '학습 모드'},
    {key: 'wrongnotes', label: '오답노트'},
    {key: 'model', label: '설정'}
]

export function Header({activePanel, onTogglePanel}) {
    return (
        <header className="app-header app__header">
            <div className="brand-block app__brand">
                <p className="eyebrow app__eyebrow">AI Speaking Coach</p>
                <h1>OMyPIc</h1>
                <p className="header-subtitle app__subtitle">말하기 연습, 질문 관리, 피드백 확인을 한 화면에서 진행하세요.</p>
            </div>

            <div className="quick-actions app__quick-actions">
                {QUICK_PANELS.map((panel) => (
                    <button
                        key={panel.key}
                        className={`quick-button app__quick-button ${activePanel === panel.key ? 'is-active' : ''}`}
                        onClick={() => onTogglePanel(panel.key)}
                    >
                        {panel.label}
                    </button>
                ))}
            </div>
        </header>
    )
}
