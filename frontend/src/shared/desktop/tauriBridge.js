// Fills window.omypic from the Tauri commands (frontend/src-tauri). Outside Tauri it does nothing,
// so the plain browser (vite dev, tests) keeps its fallbacks in shared/api/http.js.

// The default file name for the save dialog travels in this header (percent-encoded), because the
// request body is the backup itself.
const BACKUP_NAME_HEADER = 'X-Omypic-Backup-Name'

// Tauri rejects a command with the Rust error string. Screens read `error.message`, so wrap it.
function toError(reason) {
    if (reason instanceof Error) {
        return reason
    }
    return new Error(typeof reason === 'string' ? reason : String(reason))
}

// A typed array or ArrayBuffer goes over the IPC as the raw request body (no JSON). A view over
// the caller's buffer, never a copy.
function asBytes(value) {
    if (value instanceof Uint8Array) {
        return value
    }
    if (ArrayBuffer.isView(value)) {
        return new Uint8Array(value.buffer, value.byteOffset, value.byteLength)
    }
    if (Array.isArray(value)) {
        return Uint8Array.from(value)
    }
    return new Uint8Array(value)
}

// The shell answers pick_backup_file with raw bytes: the name's length (u32, little endian), the
// UTF-8 name, then the file. Empty means the dialog was cancelled. The file comes back as a view
// of the reply, not a copy.
function readPickedFile(reply) {
    const all = asBytes(reply ?? new ArrayBuffer(0))
    if (all.length === 0) {
        return null
    }
    const nameLength = all.length >= 4 ? new DataView(all.buffer, all.byteOffset, 4).getUint32(0, true) : -1
    if (nameLength < 0 || 4 + nameLength > all.length) {
        throw new Error('Malformed backup file reply from the app')
    }
    return {
        name: new TextDecoder().decode(all.subarray(4, 4 + nameLength)),
        bytes: all.subarray(4 + nameLength)
    }
}

export function installTauriBridge(target = window, invoke) {
    if (!target || !target.__TAURI_INTERNALS__) {
        return false
    }
    const call = async (command, ...args) => {
        try {
            return await invoke(command, ...args)
        } catch (reason) {
            throw toError(reason)
        }
    }
    target.omypic = {
        getBackendUrl: () => call('get_backend_url'),
        getApiKey: (provider) => call('get_api_key', {provider}),
        setApiKey: (key, provider) => call('set_api_key', {key, provider}),
        saveBackupFile: (defaultName, bytes) => call('save_backup_file', asBytes(bytes), {
            headers: {[BACKUP_NAME_HEADER]: encodeURIComponent(defaultName ?? '')}
        }),
        pickBackupFile: async () => readPickedFile(await call('pick_backup_file'))
    }
    return true
}
