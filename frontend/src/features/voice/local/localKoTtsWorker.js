// Synthesizes Korean speech on this device (ONNX Runtime WebAssembly), so Korean feedback is read
// by a Korean voice. No API is called. Messages are the same as localTtsWorker.js.
import {env, pipeline} from '@huggingface/transformers'
import {ortWasmPaths} from './ortWasm.js'
import {isVoiceFile} from './backendModelHost.js'
import {createSynthesisQueue} from './synthesisQueue.js'

let ttsPromise = null
const queue = createSynthesisQueue()

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
    // A tag inside the sentence would break it the same way, so brackets are taken out.
    const sentence = String(text).replace(/[<>]/g, '')
    const output = await tts(`<ko>${sentence}</ko>`, {speaker_embeddings: voice, ...config.options})
    return {samples: output.audio, sampleRate: output.sampling_rate}
}

self.onmessage = async (event) => {
    const {id, type, payload} = event.data || {}
    if (type === 'cancel') {
        queue.cancelThrough(payload?.through)
        return
    }
    try {
        const model = await load(payload.config)
        if (type === 'load') {
            self.postMessage({id, ok: true})
            return
        }
        const output = await queue.enqueue(id, () => synthesize(model, payload))
        if (!output) {
            self.postMessage({id, ok: false, cancelled: true})
            return
        }
        const {samples, sampleRate} = output
        self.postMessage({id, ok: true, samples, sampleRate}, [samples.buffer])
    } catch (error) {
        self.postMessage({id, ok: false, message: error?.message || String(error)})
    }
}
