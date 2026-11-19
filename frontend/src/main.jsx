import React from 'react'
import {createRoot} from 'react-dom/client'
import App from './App.jsx'
import {installTauriBridge} from './shared/desktop/tauriBridge.js'
import './styles/index.css'

// The Tauri API is loaded only inside the desktop app; the plain browser never touches it.
async function connectDesktopShell() {
    if (!window.__TAURI_INTERNALS__) {
        return
    }
    const {invoke} = await import('@tauri-apps/api/core')
    installTauriBridge(window, invoke)
}

connectDesktopShell().catch((error) => {
    console.error('Failed to connect to the desktop app', error)
}).finally(() => {
    const root = createRoot(document.getElementById('root'))
    root.render(<App/>)
})
