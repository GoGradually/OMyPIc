// Client for the on-device speech recognizer (localSttWorker.js).
import {getBackendUrl} from '../../../shared/api/http.js'
import {createSpeechWorkerClient} from './speechWorkerClient.js'

const WHISPER_SAMPLE_RATE = 16000
const DEFAULT_MODEL = 'Xenova/whisper-base.en'

function readEnv(name) {
    return (import.meta.env && import.meta.env[name]) || ''
}

// Without an override the backend downloads the model files and keeps them on disk, so the
// worker turns off the library's browser cache for those loads (backendHosted).
async function buildConfig() {
    const hostOverride = readEnv('VITE_OMYPIC_LOCAL_STT_HOST')
    return {
        model: readEnv('VITE_OMYPIC_LOCAL_STT_MODEL') || DEFAULT_MODEL,
        modelHost: hostOverride || `${await getBackendUrl()}/api/local-models/`,
        modelPathTemplate: readEnv('VITE_OMYPIC_LOCAL_STT_PATH_TEMPLATE'),
        backendHosted: !hostOverride
    }
}

// A transcription may wait this long for the worker's answer before the worker counts as hung:
// a fixed allowance plus a multiple of the audio length (answers are up to 2.5 minutes).
const STT_TIMEOUT_BASE_MS = 120000
const STT_TIMEOUT_PER_AUDIO_SECOND_MS = 4000

export function transcriptionTimeoutMs(sampleCount) {
    return STT_TIMEOUT_BASE_MS + Math.ceil(sampleCount / WHISPER_SAMPLE_RATE) * STT_TIMEOUT_PER_AUDIO_SECOND_MS
}

const client = createSpeechWorkerClient({
    createWorker: () => new Worker(new URL('./localSttWorker.js', import.meta.url), {type: 'module'}),
    buildConfig,
    texts: {
        unsupported: '이 환경에서는 로컬 음성 인식을 실행할 수 없습니다.',
        crashed: '로컬 음성 인식 모듈을 시작하지 못했습니다.',
        failed: '로컬 음성 인식에 실패했습니다.',
        timedOut: '로컬 음성 인식이 응답하지 않아 다시 시작합니다.'
    }
})

export function isLocalSttSupported() {
    return client.isSupported()
}

/** 'idle', 'loading', 'ready' or 'failed'. */
export function getLocalSttStatus() {
    return client.getStatus()
}

/**
 * Downloads (first run) and initializes the model. Safe to call repeatedly.
 * onProgress receives {file, progress} with progress in percent while files download.
 */
export function loadLocalStt(onProgress) {
    return client.load(onProgress)
}

/** Terminates the Whisper worker and frees its model unless it is in use; returns whether it did. */
export function releaseLocalStt() {
    return client.release()
}

/** True while the Whisper model loads or transcribes. */
export function isLocalSttBusy() {
    return client.isBusy()
}

export function pcm16ToFloat32(pcm16Bytes, sampleRate = WHISPER_SAMPLE_RATE) {
    const view = new DataView(pcm16Bytes.buffer, pcm16Bytes.byteOffset, pcm16Bytes.byteLength - (pcm16Bytes.byteLength % 2))
    const source = new Float32Array(view.byteLength / 2)
    for (let index = 0; index < source.length; index += 1) {
        source[index] = view.getInt16(index * 2, true) / 32768
    }
    if (!sampleRate || sampleRate === WHISPER_SAMPLE_RATE) {
        return source
    }
    // Whisper expects 16 kHz. The capture context asks for it, but a device may refuse.
    const ratio = sampleRate / WHISPER_SAMPLE_RATE
    const resampled = new Float32Array(Math.floor(source.length / ratio))
    for (let index = 0; index < resampled.length; index += 1) {
        const position = index * ratio
        const left = Math.floor(position)
        const right = Math.min(left + 1, source.length - 1)
        resampled[index] = source[left] + (source[right] - source[left]) * (position - left)
    }
    return resampled
}

export async function transcribePcm16(pcm16Bytes, sampleRate) {
    await loadLocalStt()
    const samples = pcm16ToFloat32(pcm16Bytes, sampleRate)
    if (!samples.length) {
        return ''
    }
    // The samples are moved to the worker, not copied.
    const result = await client.run('transcribe', (config) => ({config, samples}), {
        timeoutMs: transcriptionTimeoutMs(samples.length),
        makeTransfer: (payload) => [payload.samples.buffer]
    })
    return result.text || ''
}
