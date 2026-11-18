// Synthesizes English speech on this device with the Kokoro model (ONNX Runtime WebAssembly),
// so questions are read in a native voice even when the OS has no English voice. No API is called.
import {env} from '@huggingface/transformers'
import {KokoroTTS} from 'kokoro-js'
// Same bundled ONNX Runtime binary as the speech recognizer (see localSttWorker.js).
import ortFactoryUrl from 'onnxruntime-web/ort-wasm-simd-threaded.asyncify.mjs?url'
import ortWasmUrl from 'onnxruntime-web/ort-wasm-simd-threaded.asyncify.wasm?url'

let ttsPromise = null

function configure({modelHost, modelPathTemplate}) {
    env.allowLocalModels = false
    if (modelHost) {
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
        ttsPromise = KokoroTTS.from_pretrained(config.model, {
            dtype: 'q8',
            device: 'wasm',
            progress_callback: reportProgress
        }).catch((error) => {
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
