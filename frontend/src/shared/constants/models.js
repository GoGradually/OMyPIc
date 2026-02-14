export const FEEDBACK_MODELS = ['gpt-4o-mini', 'gpt-4o']

export const VOICE_STT_MODELS = ['gpt-4o-mini-transcribe', 'gpt-4o-transcribe']

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
