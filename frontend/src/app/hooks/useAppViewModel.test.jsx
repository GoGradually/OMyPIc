/** @vitest-environment jsdom */
import {afterEach, beforeEach, describe, expect, it, vi} from 'vitest'
import {act, cleanup, renderHook, waitFor} from '@testing-library/react'
import {useAppViewModel} from './useAppViewModel.js'

function okJson(body) {
    return {ok: true, json: async () => body, text: async () => JSON.stringify(body), blob: async () => new Blob()}
}

describe('useAppViewModel API key saving', () => {
    beforeEach(() => {
        global.fetch = vi.fn(async () => okJson([]))
        window.omypic = {
            getBackendUrl: vi.fn(async () => 'http://backend'),
            getApiKey: vi.fn(async () => null),
            // What the Tauri bridge does when the OS keychain cannot store the key.
            setApiKey: vi.fn(async () => {
                throw new Error('Secret Service is not available')
            })
        }
    })

    afterEach(() => {
        cleanup()
        delete window.omypic
        vi.restoreAllMocks()
    })

    it('shows the keychain error instead of failing silently', async () => {
        const {result} = renderHook(() => useAppViewModel())
        await waitFor(() => expect(window.omypic.getApiKey).toHaveBeenCalled())
        act(() => result.current.overlayContentProps.modelSettingsProps.setApiKeyInput('sk-new'))

        await act(async () => {
            await result.current.overlayContentProps.modelSettingsProps.onSaveApiKey()
        })

        expect(result.current.statusBarProps.statusMessage).toContain('OpenAI API Key 저장 실패: Secret Service is not available')
    })
})

describe('useAppViewModel data import confirmation', () => {
    let originalConfirm

    beforeEach(() => {
        originalConfirm = window.confirm
        global.fetch = vi.fn(async () => okJson([]))
        window.omypic = {
            getBackendUrl: vi.fn(async () => 'http://backend'),
            getApiKey: vi.fn(async () => null),
            setApiKey: vi.fn(async () => true),
            pickBackupFile: vi.fn(async () => null)
        }
    })

    afterEach(() => {
        cleanup()
        window.confirm = originalConfirm
        delete window.omypic
        vi.restoreAllMocks()
    })

    async function importWith(confirm) {
        window.confirm = confirm
        const {result} = renderHook(() => useAppViewModel())
        await waitFor(() => expect(window.omypic.getApiKey).toHaveBeenCalled())
        await act(async () => {
            await result.current.overlayContentProps.modelSettingsProps.onImportData()
        })
        return result
    }

    it('does not import when the learner declines an asynchronous confirm (Tauri)', async () => {
        await importWith(vi.fn(async () => false))

        expect(window.confirm).toHaveBeenCalled()
        expect(window.omypic.pickBackupFile).not.toHaveBeenCalled()
    })

    it('does not import when the learner declines a synchronous confirm (browser)', async () => {
        await importWith(vi.fn(() => false))

        expect(window.omypic.pickBackupFile).not.toHaveBeenCalled()
    })

    it('opens the file picker after the learner accepts', async () => {
        const result = await importWith(vi.fn(async () => true))

        expect(window.omypic.pickBackupFile).toHaveBeenCalledTimes(1)
        expect(result.current.statusBarProps.statusMessage).toBe('데이터 가져오기를 취소했습니다.')
    })
})
