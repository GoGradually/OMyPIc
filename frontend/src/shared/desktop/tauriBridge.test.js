import {describe, expect, it, vi} from 'vitest'
import {installTauriBridge} from './tauriBridge.js'

function tauriTarget() {
    return {__TAURI_INTERNALS__: {}}
}

function installWith(invokeImpl) {
    const target = tauriTarget()
    const invoke = vi.fn(invokeImpl)
    expect(installTauriBridge(target, invoke)).toBe(true)
    return {bridge: target.omypic, invoke}
}

describe('installTauriBridge', () => {
    it('does nothing outside Tauri', () => {
        const target = {}
        const invoke = vi.fn()

        expect(installTauriBridge(target, invoke)).toBe(false)
        expect(target.omypic).toBeUndefined()
        expect(invoke).not.toHaveBeenCalled()
    })

    it('getBackendUrl returns the address from the shell', async () => {
        const {bridge, invoke} = installWith(async () => 'http://127.0.0.1:38491')

        expect(await bridge.getBackendUrl()).toBe('http://127.0.0.1:38491')
        expect(invoke).toHaveBeenCalledWith('get_backend_url')
    })

    it('getApiKey forwards provider', async () => {
        const {bridge, invoke} = installWith(async () => 'sk-ant')

        expect(await bridge.getApiKey('anthropic')).toBe('sk-ant')
        expect(invoke).toHaveBeenCalledWith('get_api_key', {provider: 'anthropic'})
    })

    it('setApiKey forwards key and provider', async () => {
        const {bridge, invoke} = installWith(async () => true)

        expect(await bridge.setApiKey('sk-new', 'openai')).toBe(true)
        expect(invoke).toHaveBeenCalledWith('set_api_key', {key: 'sk-new', provider: 'openai'})
    })

    it('turns a string rejection into an Error carrying the message', async () => {
        const {bridge} = installWith(async () => {
            throw 'Secret Service is not available'
        })

        const error = await bridge.setApiKey('sk-new', 'openai').catch((caught) => caught)
        expect(error).toBeInstanceOf(Error)
        expect(error.message).toBe('Secret Service is not available')
    })

    it('saveBackupFile sends the bytes as the raw request body, uncopied, and returns false on cancel', async () => {
        const {bridge, invoke} = installWith(async () => false)
        const bytes = new Uint8Array([80, 75, 3])

        const saved = await bridge.saveBackupFile('backup.zip', bytes)

        expect(saved).toBe(false)
        const [command, body, options] = invoke.mock.calls[0]
        expect(command).toBe('save_backup_file')
        // A typed array goes over the IPC as raw bytes; a plain number array would become JSON text.
        expect(body).toBe(bytes)
        expect(options).toEqual({headers: {'X-Omypic-Backup-Name': 'backup.zip'}})
    })

    it('saveBackupFile accepts an ArrayBuffer without copying it', async () => {
        const {bridge, invoke} = installWith(async () => true)
        const buffer = new Uint8Array([1, 2]).buffer

        expect(await bridge.saveBackupFile('backup.zip', buffer)).toBe(true)
        const body = invoke.mock.calls[0][1]
        expect(body).toBeInstanceOf(Uint8Array)
        expect(body.buffer).toBe(buffer)
    })

    it('saveBackupFile percent-encodes the file name for the header', async () => {
        const {bridge, invoke} = installWith(async () => true)

        await bridge.saveBackupFile('백업 1.zip', new Uint8Array([1]))
        await bridge.saveBackupFile(undefined, new Uint8Array([1]))

        expect(invoke.mock.calls[0][2].headers['X-Omypic-Backup-Name']).toBe(encodeURIComponent('백업 1.zip'))
        expect(invoke.mock.calls[1][2].headers['X-Omypic-Backup-Name']).toBe('')
    })

    it('pickBackupFile returns null on cancel (empty reply)', async () => {
        const {bridge, invoke} = installWith(async () => new ArrayBuffer(0))

        expect(await bridge.pickBackupFile()).toBeNull()
        expect(invoke).toHaveBeenCalledWith('pick_backup_file')
    })

    it('pickBackupFile reads the name and a view of the bytes from the raw reply', async () => {
        const reply = framed('backup.zip', [80, 75, 3, 4])
        const {bridge} = installWith(async () => reply)

        const picked = await bridge.pickBackupFile()

        expect(picked.name).toBe('backup.zip')
        expect(picked.bytes).toBeInstanceOf(Uint8Array)
        expect(Array.from(picked.bytes)).toEqual([80, 75, 3, 4])
        expect(picked.bytes.buffer).toBe(reply)
    })

    it('pickBackupFile reads a UTF-8 name and also accepts a number array reply', async () => {
        const reply = Array.from(new Uint8Array(framed('백업.zip', [9])))
        const {bridge} = installWith(async () => reply)

        const picked = await bridge.pickBackupFile()

        expect(picked.name).toBe('백업.zip')
        expect(Array.from(picked.bytes)).toEqual([9])
    })

    it('pickBackupFile rejects a reply that is too short for its name', async () => {
        const reply = framed('backup.zip', []).slice(0, 6)
        const {bridge} = installWith(async () => reply)

        await expect(bridge.pickBackupFile()).rejects.toThrow('backup file reply')
    })
})

/** The shell's reply: name length (u32, little endian), the UTF-8 name, then the file bytes. */
function framed(name, bytes) {
    const nameBytes = new TextEncoder().encode(name)
    const out = new Uint8Array(4 + nameBytes.length + bytes.length)
    new DataView(out.buffer).setUint32(0, nameBytes.length, true)
    out.set(nameBytes, 4)
    out.set(bytes, 4 + nameBytes.length)
    return out.buffer
}
