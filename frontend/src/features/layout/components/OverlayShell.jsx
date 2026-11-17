import React, {useEffect} from 'react'

export function OverlayShell({title, onClose, children}) {
    useEffect(() => {
        const closeOnEscape = (event) => {
            if (event.key === 'Escape') {
                onClose()
            }
        }
        window.addEventListener('keydown', closeOnEscape)
        return () => window.removeEventListener('keydown', closeOnEscape)
    }, [onClose])

    return (
        <div className="overlay" onClick={onClose}>
            <div
                className="overlay-body"
                role="dialog"
                aria-modal="true"
                aria-label={title}
                onClick={(event) => event.stopPropagation()}
            >
                <div className="overlay-header">
                    <h2>{title}</h2>
                    <button className="ghost-button" onClick={onClose}>닫기</button>
                </div>
                {children}
            </div>
        </div>
    )
}
