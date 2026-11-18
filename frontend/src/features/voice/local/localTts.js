// Client for the on-device English speech synthesizer (localTtsWorker.js).

const DEFAULT_MODEL = 'onnx-community/Kokoro-82M-v1.0-ONNX'
const DEFAULT_VOICE = 'af_heart'

let worker = null
let nextRequestId = 0
let loadPromise = null
const pending = new Map()
const progressListeners = new Set()

function readEnv(name) {
    return (import.meta.env && import.meta.env[name]) || ''
}

function buildConfig() {
    return {
        model: readEnv('VITE_OMYPIC_LOCAL_TTS_MODEL') || DEFAULT_MODEL,
        voice: readEnv('VITE_OMYPIC_LOCAL_TTS_VOICE') || DEFAULT_VOICE,
        modelHost: readEnv('VITE_OMYPIC_LOCAL_TTS_HOST'),
        modelPathTemplate: readEnv('VITE_OMYPIC_LOCAL_TTS_PATH_TEMPLATE')
    }
}

export function isLocalTtsSupported() {
    return typeof Worker !== 'undefined' && typeof WebAssembly !== 'undefined'
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
    } else {
        request.reject(new Error(data.message || '로컬 음성 합성에 실패했습니다.'))
    }
}

function failAll(message) {
    pending.forEach((request) => request.reject(new Error(message)))
    pending.clear()
    loadPromise = null
    if (worker) {
        worker.terminate()
        worker = null
    }
}

function ensureWorker() {
    if (!worker) {
        worker = new Worker(new URL('./localTtsWorker.js', import.meta.url), {type: 'module'})
        worker.onmessage = handleMessage
        worker.onerror = (event) => failAll(event?.message || '로컬 음성 합성 모듈을 시작하지 못했습니다.')
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
 * Downloads (first run) and initializes the model. Safe to call repeatedly.
 * onProgress receives {file, progress} with progress in percent while files download.
 */
export function loadLocalTts(onProgress) {
    if (!isLocalTtsSupported()) {
        return Promise.reject(new Error('이 환경에서는 로컬 음성 합성을 실행할 수 없습니다.'))
    }
    if (onProgress) {
        progressListeners.add(onProgress)
    }
    if (!loadPromise) {
        loadPromise = request('load', {config: buildConfig()}).catch((error) => {
            loadPromise = null
            throw error
        })
    }
    return loadPromise.finally(() => {
        if (onProgress) {
            progressListeners.delete(onProgress)
        }
    })
}

/** Returns {samples: Float32Array, sampleRate} for one English sentence. */
export async function synthesizeEnglish(text) {
    await loadLocalTts()
    const result = await request('synthesize', {config: buildConfig(), text})
    return {samples: result.samples, sampleRate: result.sampleRate}
}
