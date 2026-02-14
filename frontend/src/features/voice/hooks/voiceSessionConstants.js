export const INITIAL_AUDIO_DEVICE_STATUS = {
    inputCount: 0,
    outputCount: 0,
    activeInputLabel: '',
    liveInput: false,
    lastCheckedAt: ''
}

export const AUDIO_PROCESS_BUFFER_SIZE = 2048
export const DEFAULT_SAMPLE_RATE = 16000
export const VAD_RMS_THRESHOLD = 0.018
export const VAD_SILENCE_MS = 1500
export const MIN_TURN_DURATION_MS = 300
export const NO_WINDOW_ID = -1

export const SPEECH_STATE = {
    IDLE: 'IDLE',
    WAITING_TTS: 'WAITING_TTS',
    PLAYING_FEEDBACK_TTS: 'PLAYING_FEEDBACK_TTS',
    PLAYING_QUESTION_TTS: 'PLAYING_QUESTION_TTS',
    READY_FOR_ANSWER: 'READY_FOR_ANSWER',
    CAPTURING_ANSWER: 'CAPTURING_ANSWER',
    STOPPING: 'STOPPING'
}
