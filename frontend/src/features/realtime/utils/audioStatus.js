import {AUDIO_PERMISSION_LABELS} from '../../../shared/constants/models.js'

export function getAudioUiState({audioPermission, audioDeviceStatus, recording}) {
    const hasConnectedAudioInput = audioDeviceStatus.liveInput || audioDeviceStatus.inputCount > 0
    const audioConnectionReady = audioPermission === 'granted' && hasConnectedAudioInput

    let audioConnectionLabel = '마이크 미연결'
    if (audioPermission === 'unsupported') {
        audioConnectionLabel = '오디오 미지원'
    } else if (recording) {
        audioConnectionLabel = '마이크 사용 중'
    } else if (hasConnectedAudioInput) {
        audioConnectionLabel = '마이크 연결됨'
    }

    const audioPermissionLabel = AUDIO_PERMISSION_LABELS[audioPermission] || AUDIO_PERMISSION_LABELS.unknown

    let audioQuickHint = '권한 요청 후 연결 상태를 확인할 수 있습니다.'
    if (audioPermission === 'unsupported') {
        audioQuickHint = '현재 환경에서는 장치 상태 확인을 지원하지 않습니다.'
    } else if (audioPermission === 'granted') {
        audioQuickHint = `입력 ${audioDeviceStatus.inputCount}개 · 출력 ${audioDeviceStatus.outputCount}개`
    }

    return {
        hasConnectedAudioInput,
        audioConnectionReady,
        audioConnectionLabel,
        audioPermissionLabel,
        audioQuickHint
    }
}
