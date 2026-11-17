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
        <header className="app-header">
            <div className="brand">
                <h1>OMyPIc</h1>
                <p>오픽 말하기 연습</p>
            </div>

            <nav className="app-nav" aria-label="관리 메뉴">
                {QUICK_PANELS.map((panel) => (
                    <button
                        key={panel.key}
                        className={`app-nav__item ${activePanel === panel.key ? 'is-active' : ''}`}
                        aria-pressed={activePanel === panel.key}
                        onClick={() => onTogglePanel(panel.key)}
                    >
                        {panel.label}
                    </button>
                ))}
            </nav>
        </header>
    )
}
