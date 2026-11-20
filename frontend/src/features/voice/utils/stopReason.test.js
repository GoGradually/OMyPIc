import {describe, expect, it} from 'vitest'
import {stoppedByServerMessage} from './stopReason.js'

describe('stoppedByServerMessage', () => {
    it('explains an idle timeout in words instead of showing the code', () => {
        const message = stoppedByServerMessage('idle_timeout')

        expect(message).toBe('오랫동안 연결이 끊겨 있어 세션이 종료되었습니다. 다시 시작해 주세요.')
        expect(message).not.toContain('idle_timeout')
    })

    it('keeps an unknown reason visible', () => {
        expect(stoppedByServerMessage('server_restart')).toBe('세션이 종료되었습니다. (server_restart)')
    })

    it('has a plain message when the server gives no reason', () => {
        expect(stoppedByServerMessage('')).toBe('세션이 서버에서 종료되었습니다.')
        expect(stoppedByServerMessage(undefined)).toBe('세션이 서버에서 종료되었습니다.')
    })
})
