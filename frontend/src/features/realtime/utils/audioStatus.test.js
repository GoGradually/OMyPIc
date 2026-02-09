import {describe, expect, it} from 'vitest'
import {getAudioUiState} from './audioStatus.js'

function status(inputCount = 0, outputCount = 0, liveInput = false) {
    return {
        inputCount,
        outputCount,
        activeInputLabel: '',
        liveInput,
        lastCheckedAt: ''
    }
}

describe('audioStatus', () => {
    it('returns unsupported labels when audio is unsupported', () => {
        const ui = getAudioUiState({
            audioPermission: 'unsupported',
            audioDeviceStatus: status(),
            recording: false
        })

        expect(ui.audioConnectionLabel).toBe('오디오 미지원')
        expect(ui.audioPermissionLabel).toBe('미지원')
        expect(ui.audioQuickHint).toBe('현재 환경에서는 장치 상태 확인을 지원하지 않습니다.')
        expect(ui.audioConnectionReady).toBe(false)
    })

    it('returns recording label while recording', () => {
        const ui = getAudioUiState({
            audioPermission: 'granted',
            audioDeviceStatus: status(1, 1, true),
            recording: true
        })

        expect(ui.audioConnectionLabel).toBe('마이크 사용 중')
        expect(ui.audioConnectionReady).toBe(true)
    })

    it('shows quick hint with input and output counts', () => {
        const ui = getAudioUiState({
            audioPermission: 'granted',
            audioDeviceStatus: status(2, 1, false),
            recording: false
        })

        expect(ui.audioQuickHint).toBe('입력 2개 · 출력 1개')
    })
})
