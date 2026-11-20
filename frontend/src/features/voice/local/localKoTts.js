// Client for the on-device Korean speech synthesizer (localKoTtsWorker.js). It runs in its own
// worker, so a long first download or a failure of this model never holds up English speech.
import {getBackendUrl} from '../../../shared/api/http.js'

const KO_TTS_MODEL = 'onnx-community/Supertonic-TTS-2-ONNX'
// The only dtype this repository has; another one makes the library look for files that are not there.
const KO_TTS_DTYPE = 'fp32'
const KO_TTS_VOICE = 'F2'
const KO_TTS_OPTIONS = {num_inference_steps: 5, speed: 1.05}

let worker = null
let nextRequestId = 0
let loadPromise = null
// The settings the current load was started with; every sentence is synthesized with the same ones.
let loadedConfig = null
// 'idle' (never asked), 'loading', 'ready' or 'failed' (the last load failed; the next call retries).
let status = 'idle'
const pending = new Map()
const progressListeners = new Set()

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

export function isLocalKoTtsSupported() {
    return typeof Worker !== 'undefined' && typeof WebAssembly !== 'undefined'
}

/** 'idle', 'loading', 'ready' or 'failed'. */
export function getLocalKoTtsStatus() {
    return status
}

function handleMessage(event) {
    const data = event.data || {}
    if (data.type === 'progress') {
        progressListeners.forEach((listener) => listener(data))
        return
    }
    const request = pending.get(data.id)
    if (!request) {
        return
    }
    pending.delete(data.id)
    if (data.ok) {
        request.resolve(data)
    } else if (data.cancelled) {
        request.reject(Object.assign(new Error('음성 합성을 취소했습니다.'), {cancelled: true}))
    } else {
        request.reject(new Error(data.message || '한국어 음성 합성에 실패했습니다.'))
    }
}

// A crashed worker takes the loaded model with it. The status says so, so the next Korean
// sentence is shown without sound and starts a reload in the background instead of waiting.
function failAll(message) {
    pending.forEach((request) => request.reject(new Error(message)))
    pending.clear()
    loadPromise = null
    status = 'failed'
    if (worker) {
        worker.terminate?.()
        worker = null
    }
}

function ensureWorker() {
    if (!worker) {
        worker = new Worker(new URL('./localKoTtsWorker.js', import.meta.url), {type: 'module'})
        worker.onmessage = handleMessage
        worker.onerror = (event) => failAll(event?.message || '한국어 음성 합성 모듈을 시작하지 못했습니다.')
    }
    return worker
}

function request(type, payload) {
    return new Promise((resolve, reject) => {
        const id = ++nextRequestId
        pending.set(id, {resolve, reject})
        ensureWorker().postMessage({id, type, payload})
    })
}

/**
 * Downloads (first run) and initializes the Korean model. Safe to call repeatedly.
 * onProgress receives {file, progress} with progress in percent while files download.
 */
export function loadLocalKoTts(onProgress) {
    if (!isLocalKoTtsSupported()) {
        status = 'failed'
        return Promise.reject(new Error('이 환경에서는 한국어 음성 합성을 실행할 수 없습니다.'))
    }
    if (onProgress) {
        progressListeners.add(onProgress)
    }
    if (!loadPromise) {
        status = 'loading'
        loadPromise = buildConfig().then((config) => {
            loadedConfig = config
            return request('load', {config})
        }).then(() => {
            status = 'ready'
        }, (error) => {
            loadPromise = null
            status = 'failed'
            throw error
        })
    }
    return loadPromise.finally(() => {
        if (onProgress) {
            progressListeners.delete(onProgress)
        }
    })
}

/**
 * Returns {samples: Float32Array, sampleRate} for one Korean sentence. Send one splitForSpeech
 * chunk per call: the model silently drops input past its limit.
 */
export async function synthesizeKorean(text) {
    await loadLocalKoTts()
    const result = await request('synthesize', {config: loadedConfig, text})
    return {samples: result.samples, sampleRate: result.sampleRate}
}

/**
 * Drops every sentence sent so far that the worker has not started synthesizing; their promises
 * reject with an error whose `cancelled` is true. Sentences sent after this call are not affected.
 */
export function cancelQueuedKoreanSynthesis() {
    worker?.postMessage({type: 'cancel', payload: {through: nextRequestId}})
}
