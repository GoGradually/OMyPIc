export const FEEDBACK_MODEL_OPTIONS = [
    {value: 'gpt-5-pro', label: 'gpt-5-pro (최고 성능)'},
    {value: 'gpt-5.2', label: 'gpt-5.2 (최신 고성능)'},
    {value: 'gpt-5.1', label: 'gpt-5.1 (고성능)'},
    {value: 'gpt-5', label: 'gpt-5 (표준 상위)'},
    {value: 'gpt-5-mini', label: 'gpt-5-mini (균형형)'},
    {value: 'gpt-5-nano', label: 'gpt-5-nano (초저비용)'},
    {value: 'gpt-4.1', label: 'gpt-4.1 (비추론 상위)'},
    {value: 'gpt-4.1-mini', label: 'gpt-4.1-mini (비추론 균형형)'},
    {value: 'gpt-4.1-nano', label: 'gpt-4.1-nano (비추론 초저비용)'},
    {value: 'gpt-4o', label: 'gpt-4o (레거시 상위)'},
    {value: 'gpt-4o-mini', label: 'gpt-4o-mini (레거시 기본)'}
]

export const VOICE_STT_MODEL_OPTIONS = [
    {value: 'gpt-4o-transcribe', label: 'gpt-4o-transcribe (정확도 우선)'},
    {value: 'gpt-4o-mini-transcribe', label: 'gpt-4o-mini-transcribe (기본)'},
    {value: 'whisper-1', label: 'whisper-1 (호환)'}
]

export const TTS_MODEL_OPTIONS = [
    {value: 'gpt-4o-mini-tts', label: 'gpt-4o-mini-tts (기본)'},
    {value: 'tts-1-hd', label: 'tts-1-hd (품질 우선)'},
    {value: 'tts-1', label: 'tts-1 (속도 우선)'}
]

export const DEFAULT_FEEDBACK_MODEL = 'gpt-4o-mini'
export const DEFAULT_VOICE_STT_MODEL = 'gpt-4o-mini-transcribe'
export const DEFAULT_TTS_MODEL = 'gpt-4o-mini-tts'

export const FEEDBACK_MODELS = FEEDBACK_MODEL_OPTIONS.map((item) => item.value)
export const VOICE_STT_MODELS = VOICE_STT_MODEL_OPTIONS.map((item) => item.value)
export const TTS_MODELS = TTS_MODEL_OPTIONS.map((item) => item.value)

export const VOICES = ['alloy', 'echo', 'fable', 'nova', 'shimmer']

export const MODE_OPTIONS = [
    {value: 'IMMEDIATE', label: '즉시 피드백'},
    {value: 'CONTINUOUS', label: '연속 발화'}
]

export const AUDIO_PERMISSION_LABELS = {
    unknown: '확인 전',
    prompt: '권한 필요',
    granted: '허용됨',
    denied: '거부됨',
    unsupported: '미지원'
}

export const PANEL_TITLES = {
    rulebook: '룰북 관리',
    'question-group-manager': '그룹/태그 관리',
    'group-question-manager': '그룹 내 질문 관리',
    'learning-mode': '학습 모드 관리',
    wrongnotes: '오답 노트',
    model: '연결 설정'
}

export function getModeSummary(mode, batchSize) {
    if (mode === 'CONTINUOUS') {
        return `연속 발화 (${batchSize}질문 그룹 단위)`
    }
    return '즉시 피드백'
}
