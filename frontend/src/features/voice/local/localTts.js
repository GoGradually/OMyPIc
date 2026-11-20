// Client for the on-device English speech synthesizer (localTtsWorker.js).
import {getBackendUrl} from '../../../shared/api/http.js'
import {createSpeechWorkerClient} from './speechWorkerClient.js'

const DEFAULT_MODEL = 'onnx-community/Kokoro-82M-v1.0-ONNX'
const DEFAULT_VOICE = 'af_heart'
// The commit of DEFAULT_MODEL every install loads (voice files included), so an upstream change
// cannot change the voice silently.
const DEFAULT_REVISION = '1939ad2a8e416c0acfeecc08a694d14ef25f2231'
// Size of that commit's files, so the download percentage covers the whole model (0: unknown).
const DEFAULT_DOWNLOAD_BYTES = 92_887_010

function readEnv(name) {
    return (import.meta.env && import.meta.env[name]) || ''
}

// Without an override the backend downloads the model files and keeps them on disk, so the
// worker turns off the library's browser cache for those loads (backendHosted).
async function buildConfig() {
    const hostOverride = readEnv('VITE_OMYPIC_LOCAL_TTS_HOST')
    const model = readEnv('VITE_OMYPIC_LOCAL_TTS_MODEL') || DEFAULT_MODEL
    const revision = readEnv('VITE_OMYPIC_LOCAL_TTS_REVISION') || (model === DEFAULT_MODEL ? DEFAULT_REVISION : 'main')
    return {
        model,
        revision,
        downloadBytes: model === DEFAULT_MODEL && revision === DEFAULT_REVISION ? DEFAULT_DOWNLOAD_BYTES : 0,
        voice: readEnv('VITE_OMYPIC_LOCAL_TTS_VOICE') || DEFAULT_VOICE,
        modelHost: hostOverride || `${await getBackendUrl()}/api/local-models/`,
        modelPathTemplate: readEnv('VITE_OMYPIC_LOCAL_TTS_PATH_TEMPLATE'),
        backendHosted: !hostOverride
    }
}

// A synthesis request may wait this long for the worker's next answer before the worker counts as
// hung (a sentence takes about 3-9 s on a slow CPU; WebKit is slower).
export const ENGLISH_REQUEST_TIMEOUT_MS = 120000

const client = createSpeechWorkerClient({
    createWorker: () => new Worker(new URL('./localTtsWorker.js', import.meta.url), {type: 'module'}),
    buildConfig,
    texts: {
        unsupported: '이 환경에서는 로컬 음성 합성을 실행할 수 없습니다.',
        crashed: '로컬 음성 합성 모듈을 시작하지 못했습니다.',
        failed: '로컬 음성 합성에 실패했습니다.',
        timedOut: '로컬 음성 합성이 응답하지 않아 다시 시작합니다.'
    }
})

export function isLocalTtsSupported() {
    return client.isSupported()
}

/** 'idle', 'loading', 'ready' or 'failed'. */
export function getLocalTtsStatus() {
    return client.getStatus()
}

/** Calls listener({status, progress}) whenever the status or the download progress changes; returns a function that stops it. */
export function watchLocalTtsStatus(listener) {
    return client.watchStatus(listener)
}

/**
 * Downloads (first run) and initializes the model. Safe to call repeatedly.
 * onProgress receives {file, progress} with progress in percent while files download.
 */
export function loadLocalTts(onProgress) {
    return client.load(onProgress)
}

/** Returns {samples: Float32Array, sampleRate} for one English sentence. */
export async function synthesizeEnglish(text) {
    const result = await client.run('synthesize', (config) => ({config, text}), {timeoutMs: ENGLISH_REQUEST_TIMEOUT_MS})
    return {samples: result.samples, sampleRate: result.sampleRate}
}

/**
 * Drops every sentence asked for so far that the worker has not started synthesizing (also one
 * still waiting to be sent); their promises reject with an error whose `cancelled` is true.
 * Sentences asked for after this call are not affected.
 */
export function cancelQueuedEnglishSynthesis() {
    client.cancelQueued()
}

/** Terminates the English worker and frees its model unless it is in use; returns whether it did. */
export function releaseLocalTts() {
    return client.release()
}

/** True while the English model loads or synthesizes. */
export function isLocalTtsBusy() {
    return client.isBusy()
}
