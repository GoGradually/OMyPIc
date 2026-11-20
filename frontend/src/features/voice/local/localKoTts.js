// Client for the on-device Korean speech synthesizer (localKoTtsWorker.js). It runs in its own
// worker, so a long first download or a failure of this model never holds up English speech.
import {getBackendUrl} from '../../../shared/api/http.js'
import {createSpeechWorkerClient} from './speechWorkerClient.js'

const KO_TTS_MODEL = 'onnx-community/Supertonic-TTS-2-ONNX'
// The only dtype this repository has; another one makes the library look for files that are not there.
const KO_TTS_DTYPE = 'fp32'
const KO_TTS_VOICE = 'F2'
const KO_TTS_OPTIONS = {num_inference_steps: 5, speed: 1.05}

function readEnv(name) {
    return (import.meta.env && import.meta.env[name]) || ''
}

// The library fetches the voice file from the exact address it is given (not through its model
// host), so the address is built here from the same host and path layout as the model files.
function voiceUrlOf({model, voice, modelHost, modelPathTemplate, backendHosted}) {
    const path = backendHosted
        ? `${model}/resolve/main/`
        : (modelPathTemplate || '{model}/').replace('{model}', model).replace('{revision}', 'main')
    return `${modelHost}${path}voices/${voice}.bin`
}

// Same layout as localTts.js: without an override the backend downloads the model files and
// keeps them on disk, so the worker turns off the library's browser cache (backendHosted).
async function buildConfig() {
    const hostOverride = readEnv('VITE_OMYPIC_LOCAL_KO_TTS_HOST')
    const config = {
        model: readEnv('VITE_OMYPIC_LOCAL_KO_TTS_MODEL') || KO_TTS_MODEL,
        dtype: KO_TTS_DTYPE,
        voice: readEnv('VITE_OMYPIC_LOCAL_KO_TTS_VOICE') || KO_TTS_VOICE,
        options: {...KO_TTS_OPTIONS},
        modelHost: hostOverride || `${await getBackendUrl()}/api/local-models/`,
        modelPathTemplate: readEnv('VITE_OMYPIC_LOCAL_KO_TTS_PATH_TEMPLATE'),
        backendHosted: !hostOverride
    }
    return {...config, voiceUrl: voiceUrlOf(config)}
}

// A synthesis request may wait this long for the worker's next answer before the worker counts as
// hung (a 220-character sentence took 10 s on a slow CPU; WebKit is slower).
export const KOREAN_REQUEST_TIMEOUT_MS = 120000

const client = createSpeechWorkerClient({
    createWorker: () => new Worker(new URL('./localKoTtsWorker.js', import.meta.url), {type: 'module'}),
    buildConfig,
    texts: {
        unsupported: '이 환경에서는 한국어 음성 합성을 실행할 수 없습니다.',
        crashed: '한국어 음성 합성 모듈을 시작하지 못했습니다.',
        failed: '한국어 음성 합성에 실패했습니다.',
        timedOut: '한국어 음성 합성이 응답하지 않아 다시 시작합니다.'
    }
})

export function isLocalKoTtsSupported() {
    return client.isSupported()
}

/** 'idle', 'loading', 'ready' or 'failed'. */
export function getLocalKoTtsStatus() {
    return client.getStatus()
}

/** Calls listener({status, progress}) whenever the status or the download progress changes; returns a function that stops it. */
export function watchLocalKoTtsStatus(listener) {
    return client.watchStatus(listener)
}

/**
 * Downloads (first run) and initializes the Korean model. Safe to call repeatedly.
 * onProgress receives {file, progress} with progress in percent while files download.
 */
export function loadLocalKoTts(onProgress) {
    return client.load(onProgress)
}

/**
 * Returns {samples: Float32Array, sampleRate} for one Korean sentence. Send one splitForSpeech
 * chunk per call: the model silently drops input past its limit. Every sentence is synthesized
 * with the settings its load was started with.
 */
export async function synthesizeKorean(text) {
    const result = await client.run('synthesize', (config) => ({config, text}), {timeoutMs: KOREAN_REQUEST_TIMEOUT_MS})
    return {samples: result.samples, sampleRate: result.sampleRate}
}

/**
 * Drops every sentence asked for so far that the worker has not started synthesizing (also one
 * still waiting to be sent); their promises reject with an error whose `cancelled` is true.
 * Sentences asked for after this call are not affected.
 */
export function cancelQueuedKoreanSynthesis() {
    client.cancelQueued()
}

/** Terminates the Korean worker and frees its model unless it is in use; returns whether it did. */
export function releaseLocalKoTts() {
    return client.release()
}

/** True while the Korean model loads or synthesizes. */
export function isLocalKoTtsBusy() {
    return client.isBusy()
}
