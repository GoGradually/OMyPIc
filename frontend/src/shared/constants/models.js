export const MODEL_LABELS = {
    'gpt-5-mini': 'gpt-5-mini (균형형)',
    'gpt-5-nano': 'gpt-5-nano (초저비용)',
    'gpt-4.1': 'gpt-4.1 (비추론 상위)',
    'gpt-4.1-mini': 'gpt-4.1-mini (비추론 균형형)',
    'gpt-4.1-nano': 'gpt-4.1-nano (비추론 초저비용)',
    'gpt-4o': 'gpt-4o (레거시 상위)',
    'gpt-4o-mini': 'gpt-4o-mini (레거시 기본)',
    'gpt-4o-transcribe': 'gpt-4o-transcribe (정확도 우선)',
    'gpt-4o-mini-transcribe': 'gpt-4o-mini-transcribe (기본)',
    'whisper-1': 'whisper-1 (호환)',
    'gpt-4o-mini-tts': 'gpt-4o-mini-tts (기본)',
    'tts-1-hd': 'tts-1-hd (품질 우선)',
    'tts-1': 'tts-1 (속도 우선)'
}

export const FEEDBACK_LANGUAGE_LABELS = {
    ko: '한국어',
    en: 'English'
}

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

export function getModelLabel(modelId) {
    if (!modelId) {
        return ''
    }
    return MODEL_LABELS[modelId] || modelId
}

export function getModeSummary(mode, batchSize) {
    if (mode === 'CONTINUOUS') {
        return `연속 발화 (${batchSize}질문 그룹 단위)`
    }
    return '즉시 피드백'
}
