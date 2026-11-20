// Synthesizes Korean speech on this device (ONNX Runtime WebAssembly), so Korean feedback is read
// by a Korean voice. No API is called. Messages are the same as localTtsWorker.js.
import {env, pipeline} from '@huggingface/transformers'
import {ortWasmPaths} from './ortWasm.js'
import {isVoiceFile} from './backendModelHost.js'

let ttsPromise = null
// One synthesis at a time: requests for several sentences arrive together.
let queue = Promise.resolve()

function configure({modelHost, modelPathTemplate, backendHosted}) {
    env.allowLocalModels = false
    if (backendHosted) {
        // The backend serves the files from Hugging Face's own path layout and keeps them on disk,
        // so only the host changes and the browser needs no second copy.
        env.remoteHost = modelHost
        env.useBrowserCache = false
    } else if (modelHost) {
        env.remoteHost = modelHost
        env.remotePathTemplate = modelPathTemplate || '{model}/'
    }
    env.backends.onnx.wasm.wasmPaths = ortWasmPaths(self.location.href)
}

// The library reports each file and the total; the total is what tells the learner how far along
// the whole model is.
function reportProgress(event) {
    if (event?.status !== 'progress_total' || !Number.isFinite(event.progress)) {
        return
    }
    self.postMessage({type: 'progress', file: event.name || '', progress: event.progress})
}

// The voice is a small float32 style vector. It is fetched once and checked, so a failed download
// (an error body from the backend) fails the load instead of producing noise.
async function fetchVoice(url) {
    const response = await fetch(url)
    if (!response.ok) {
        throw new Error(`Voice file download failed: HTTP ${response.status} for ${url}`)
    }
    const buffer = await response.arrayBuffer()
    if (!isVoiceFile(response, buffer.byteLength)) {
        throw new Error(`Voice file is not a voice (${buffer.byteLength} bytes): ${url}`)
    }
    return new Float32Array(buffer)
}

function load(config) {
    if (!ttsPromise) {
        configure(config)
        ttsPromise = Promise.all([
            pipeline('text-to-speech', config.model, {
                device: 'wasm',
                dtype: config.dtype,
                progress_callback: reportProgress
            }),
            fetchVoice(config.voiceUrl)
        ]).then(([tts, voice]) => ({tts, voice})).catch((error) => {
            ttsPromise = null
            throw error
        })
    }
    return ttsPromise
}

async function synthesize({tts, voice}, {text, config}) {
    // The language is chosen by the tag around the whole sentence; without it Korean breaks up.
    const output = await tts(`<ko>${text}</ko>`, {speaker_embeddings: voice, ...config.options})
    return {samples: output.audio, sampleRate: output.sampling_rate}
}

self.onmessage = async (event) => {
    const {id, type, payload} = event.data || {}
    try {
        const model = await load(payload.config)
        if (type === 'load') {
            self.postMessage({id, ok: true})
            return
        }
        const run = queue.then(() => synthesize(model, payload))
        queue = run.catch(() => {})
        const {samples, sampleRate} = await run
        self.postMessage({id, ok: true, samples, sampleRate}, [samples.buffer])
    } catch (error) {
        self.postMessage({id, ok: false, message: error?.message || String(error)})
    }
}
