export const FEEDBACK_MODELS = {
    openai: ['gpt-realtime-mini', 'gpt-realtime'],
    anthropic: ['claude-3-5-sonnet-20240620', 'claude-3-haiku-20240307'],
    gemini: ['gemini-1.5-pro', 'gemini-1.5-flash']
}

export const REALTIME_CONVERSATION_MODELS = ['gpt-realtime-mini', 'gpt-realtime']

export const REALTIME_STT_MODELS = ['gpt-4o-mini-transcribe', 'gpt-4o-transcribe']

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
    questions: '질문 그룹 및 모드 설정',
    wrongnotes: '오답 노트',
    model: '연결 설정'
}

export function getModeSummary(mode, batchSize) {
    if (mode === 'CONTINUOUS') {
        return `연속 발화 (${batchSize}질문 그룹 단위)`
    }
    return '즉시 피드백'
}
