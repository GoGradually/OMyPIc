// Client for the on-device speech recognizer (localSttWorker.js).
import {getBackendUrl} from '../../../shared/api/http.js'

const WHISPER_SAMPLE_RATE = 16000
const DEFAULT_MODEL = 'Xenova/whisper-base.en'

let worker = null
let nextRequestId = 0
let loadPromise = null
const pending = new Map()
const progressListeners = new Set()

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

export function isLocalSttSupported() {
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
        request.reject(new Error(data.message || '로컬 음성 인식에 실패했습니다.'))
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
        worker = new Worker(new URL('./localSttWorker.js', import.meta.url), {type: 'module'})
        worker.onmessage = handleMessage
        worker.onerror = (event) => failAll(event?.message || '로컬 음성 인식 모듈을 시작하지 못했습니다.')
    }
    return worker
}

function request(type, payload, transfer = []) {
    return new Promise((resolve, reject) => {
        const id = ++nextRequestId
        pending.set(id, {resolve, reject})
        ensureWorker().postMessage({id, type, payload}, transfer)
    })
}

/**
 * Downloads (first run) and initializes the model. Safe to call repeatedly.
 * onProgress receives {file, progress} with progress in percent while files download.
 */
export function loadLocalStt(onProgress) {
    if (!isLocalSttSupported()) {
        return Promise.reject(new Error('이 환경에서는 로컬 음성 인식을 실행할 수 없습니다.'))
    }
    if (onProgress) {
        progressListeners.add(onProgress)
    }
    if (!loadPromise) {
        loadPromise = buildConfig().then((config) => request('load', {config})).catch((error) => {
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
    const result = await request('transcribe', {config: await buildConfig(), samples}, [samples.buffer])
    return result.text || ''
}
