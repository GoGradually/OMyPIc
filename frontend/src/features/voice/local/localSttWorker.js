// Runs Whisper on this device (WebAssembly) so answers can be transcribed without any API.
// Lives in a worker because a transcription takes seconds and would freeze the UI otherwise.
import {env, pipeline} from '@huggingface/transformers'
import {ortWasmPaths} from './ortWasm.js'

const WHISPER_SAMPLE_RATE = 16000
let transcriberPromise = null

function configure({modelHost, modelPathTemplate, backendHosted}) {
    env.allowLocalModels = false
    if (backendHosted) {
        // The backend serves the files from Hugging Face's own path layout and keeps them on disk,
        // so only the host changes and the browser needs no second copy.
        env.remoteHost = modelHost
        env.useBrowserCache = false
    } else if (modelHost) {
        // A mirror or a bundled copy of the model, laid out like the Hugging Face repository.
        env.remoteHost = modelHost
        env.remotePathTemplate = modelPathTemplate || '{model}/'
    }
    env.backends.onnx.wasm.wasmPaths = ortWasmPaths(self.location.href)
}

function reportProgress(event) {
    if (event?.status !== 'progress' || !Number.isFinite(event.progress)) {
        return
    }
    self.postMessage({type: 'progress', file: event.file || '', progress: event.progress})
}

function load(config) {
    if (!transcriberPromise) {
        configure(config)
        transcriberPromise = pipeline('automatic-speech-recognition', config.model, {
            dtype: 'q8',
            device: 'wasm',
            progress_callback: reportProgress
        }).catch((error) => {
            transcriberPromise = null
            throw error
        })
    }
    return transcriberPromise
}

async function transcribe({config, samples}) {
    const transcriber = await load(config)
    // English-only checkpoints (*.en) reject language/task options; multilingual ones need them
    // or they may answer in another language.
    const options = config.model.endsWith('.en') ? {} : {language: 'english', task: 'transcribe'}
    const seconds = samples.length / WHISPER_SAMPLE_RATE
    if (seconds > 28) {
        // Whisper reads 30-second windows; longer answers are stitched from overlapping chunks.
        options.chunk_length_s = 30
        options.stride_length_s = 5
    }
    const output = await transcriber(samples, options)
    return (Array.isArray(output) ? output[0]?.text : output?.text) || ''
}

self.onmessage = async (event) => {
    const {id, type, payload} = event.data || {}
    try {
        if (type === 'load') {
            await load(payload.config)
            self.postMessage({id, ok: true})
        } else if (type === 'transcribe') {
            const text = await transcribe(payload)
            self.postMessage({id, ok: true, text: text.trim()})
        }
    } catch (error) {
        self.postMessage({id, ok: false, message: error?.message || String(error)})
    }
}
