import {describe, expect, it, vi} from 'vitest'
import {confirmAction} from './confirmAction.js'

describe('confirmAction', () => {
    it('returns the answer of a synchronous confirm (plain browser)', async () => {
        expect(await confirmAction('계속?', {confirm: vi.fn(() => true)})).toBe(true)
        expect(await confirmAction('계속?', {confirm: vi.fn(() => false)})).toBe(false)
    })

    it('waits for an asynchronous confirm (Tauri dialog plugin)', async () => {
        const declined = {confirm: vi.fn(async () => false)}
        const accepted = {confirm: vi.fn(async () => true)}

        expect(await confirmAction('계속?', declined)).toBe(false)
        expect(await confirmAction('계속?', accepted)).toBe(true)
        expect(declined.confirm).toHaveBeenCalledWith('계속?')
    })

    it('treats a failed or missing confirm as a refusal', async () => {
        const logged = vi.spyOn(console, 'error').mockImplementation(() => {})
        const failing = {
            confirm: vi.fn(async () => {
                throw 'Command plugin:dialog|message not allowed by ACL'
            })
        }

        expect(await confirmAction('계속?', failing)).toBe(false)
        expect(await confirmAction('계속?', {})).toBe(false)
        expect(await confirmAction('계속?', {confirm: vi.fn(() => undefined)})).toBe(false)
        expect(logged).toHaveBeenCalledTimes(1)
        logged.mockRestore()
    })
})
