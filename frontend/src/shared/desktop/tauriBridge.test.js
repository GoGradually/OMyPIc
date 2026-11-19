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

    it('saveBackupFile sends plain number array and returns false on cancel', async () => {
        const {bridge, invoke} = installWith(async () => false)

        const saved = await bridge.saveBackupFile('backup.zip', new Uint8Array([80, 75, 3]))

        expect(saved).toBe(false)
        expect(invoke).toHaveBeenCalledWith('save_backup_file', {defaultName: 'backup.zip', bytes: [80, 75, 3]})
        const sent = invoke.mock.calls[0][1].bytes
        expect(Array.isArray(sent)).toBe(true)
    })

    it('saveBackupFile accepts an ArrayBuffer', async () => {
        const {bridge, invoke} = installWith(async () => true)

        expect(await bridge.saveBackupFile('backup.zip', new Uint8Array([1, 2]).buffer)).toBe(true)
        expect(invoke.mock.calls[0][1].bytes).toEqual([1, 2])
    })

    it('pickBackupFile returns null on cancel', async () => {
        const {bridge, invoke} = installWith(async () => null)

        expect(await bridge.pickBackupFile()).toBeNull()
        expect(invoke).toHaveBeenCalledWith('pick_backup_file')
    })

    it('pickBackupFile converts bytes to Uint8Array', async () => {
        const {bridge} = installWith(async () => ({name: 'backup.zip', bytes: [80, 75, 3, 4]}))

        const picked = await bridge.pickBackupFile()

        expect(picked.name).toBe('backup.zip')
        expect(picked.bytes).toBeInstanceOf(Uint8Array)
        expect(Array.from(picked.bytes)).toEqual([80, 75, 3, 4])
    })
})
