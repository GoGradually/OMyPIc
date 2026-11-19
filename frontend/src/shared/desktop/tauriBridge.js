// Fills window.omypic from the Tauri commands (frontend/src-tauri). Outside Tauri it does nothing,
// so the plain browser (vite dev, tests) keeps its fallbacks in shared/api/http.js.

// Tauri rejects a command with the Rust error string. Screens read `error.message`, so wrap it.
function toError(reason) {
    if (reason instanceof Error) {
        return reason
    }
    return new Error(typeof reason === 'string' ? reason : String(reason))
}

export function installTauriBridge(target = window, invoke) {
    if (!target || !target.__TAURI_INTERNALS__) {
        return false
    }
    const call = async (command, args) => {
        try {
            return args === undefined ? await invoke(command) : await invoke(command, args)
        } catch (reason) {
            throw toError(reason)
        }
    }
    target.omypic = {
        getBackendUrl: () => call('get_backend_url'),
        getApiKey: (provider) => call('get_api_key', {provider}),
        setApiKey: (key, provider) => call('set_api_key', {key, provider}),
        // Rust takes Vec<u8> as a JSON number array; a Uint8Array would serialize as an object.
        saveBackupFile: (defaultName, bytes) => call('save_backup_file', {
            defaultName,
            bytes: Array.from(new Uint8Array(bytes))
        }),
        pickBackupFile: async () => {
            const picked = await call('pick_backup_file')
            return picked ? {name: picked.name, bytes: new Uint8Array(picked.bytes)} : null
        }
    }
    return true
}
