// Synthesizes English speech on this device with the Kokoro model (ONNX Runtime WebAssembly),
// so questions are read in a native voice even when the OS has no English voice. No API is called.
import {env} from '@huggingface/transformers'
import {KokoroTTS} from 'kokoro-js'
import {dropBrokenVoiceCache, redirectHuggingFaceFetch} from './backendModelHost.js'
// Same bundled ONNX Runtime binary as the speech recognizer (see localSttWorker.js).
import ortFactoryUrl from 'onnxruntime-web/ort-wasm-simd-threaded.asyncify.mjs?url'
import ortWasmUrl from 'onnxruntime-web/ort-wasm-simd-threaded.asyncify.wasm?url'

let ttsPromise = null

function configure({modelHost, modelPathTemplate, backendHosted}) {
    env.allowLocalModels = false
    if (backendHosted) {
        // The backend serves the files from Hugging Face's own path layout and keeps them on disk,
        // so only the host changes and the browser needs no second copy.
        env.remoteHost = modelHost
        env.useBrowserCache = false
        // kokoro-js fetches voices/*.bin from a fixed Hugging Face address; send that to the backend too.
        redirectHuggingFaceFetch(self, modelHost)
    } else if (modelHost) {
        env.remoteHost = modelHost
        env.remotePathTemplate = modelPathTemplate || '{model}/'
    }
    env.backends.onnx.wasm.wasmPaths = {
        mjs: new URL(ortFactoryUrl, self.location.href).href,
        wasm: new URL(ortWasmUrl, self.location.href).href
    }
}

function reportProgress(event) {
    if (event?.status !== 'progress' || !Number.isFinite(event.progress)) {
        return
    }
    self.postMessage({type: 'progress', file: event.file || '', progress: event.progress})
}

function load(config) {
    if (!ttsPromise) {
        configure(config)
        // A failed voice download stored by an earlier run would otherwise be read back every time.
        ttsPromise = dropBrokenVoiceCache(self.caches).then(() => KokoroTTS.from_pretrained(config.model, {
            dtype: 'q8',
            device: 'wasm',
            progress_callback: reportProgress
        })).catch((error) => {
            ttsPromise = null
            throw error
        })
    }
    return ttsPromise
}

self.onmessage = async (event) => {
    const {id, type, payload} = event.data || {}
    try {
        const tts = await load(payload.config)
        if (type === 'load') {
            self.postMessage({id, ok: true})
            return
        }
        const output = await tts.generate(payload.text, {voice: payload.config.voice})
        const samples = output.audio
        self.postMessage({id, ok: true, samples, sampleRate: output.sampling_rate}, [samples.buffer])
    } catch (error) {
        self.postMessage({id, ok: false, message: error?.message || String(error)})
    }
}
