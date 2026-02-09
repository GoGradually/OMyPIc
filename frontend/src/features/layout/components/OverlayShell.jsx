import React from 'react'

export function OverlayShell({title, onClose, children}) {
    return (
        <div className="overlay app-overlay" onClick={onClose}>
            <div className="overlay-body app-overlay__body" onClick={(event) => event.stopPropagation()}>
                <div className="overlay-header app-overlay__header">
                    <h2>{title}</h2>
                    <button className="ghost-button" onClick={onClose}>닫기</button>
                </div>
                {children}
            </div>
        </div>
    )
}
