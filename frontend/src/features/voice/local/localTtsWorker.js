// Synthesizes English speech on this device with the Kokoro model (ONNX Runtime WebAssembly),
// so questions are read in a native English voice. No API is called.
import {env} from '@huggingface/transformers'
import {KokoroTTS} from 'kokoro-js'
import {dropBrokenVoiceCache, KOKORO_VOICE_CACHE, modelPathTemplate, NO_CACHE_STORAGE, redirectHuggingFaceFetch} from './backendModelHost.js'
import {ortWasmPaths} from './ortWasm.js'
import {createSynthesisQueue} from './synthesisQueue.js'

let ttsPromise = null
// The worker's own Cache Storage, before configure() hides it from kokoro-js.
let browserCaches = null
const queue = createSynthesisQueue()

function configure(config) {
    env.allowLocalModels = false
    if (config.backendHosted) {
        // The backend serves the files from Hugging Face's own path layout and keeps them on disk,
        // so only the host changes and the browser needs no second copy.
        env.remoteHost = config.modelHost
        env.useBrowserCache = false
        // kokoro-js fetches voices/*.bin from a fixed Hugging Face `main` address; send that to the
        // backend at the pinned revision, and keep no copy of it in Cache Storage.
        redirectHuggingFaceFetch(self, config.modelHost, config.revision)
        browserCaches = self.caches
        Object.defineProperty(self, 'caches', {value: NO_CACHE_STORAGE, configurable: true})
    } else if (config.modelHost) {
        env.remoteHost = config.modelHost
    }
    // kokoro-js does not pass a revision on, so the pinned revision is written into the path.
    env.remotePathTemplate = modelPathTemplate(config)
    env.backends.onnx.wasm.wasmPaths = ortWasmPaths(self.location.href)
}

function reportProgress(event) {
    if (event?.status !== 'progress' || !Number.isFinite(event.progress)) {
        return
    }
    self.postMessage({type: 'progress', file: event.file || '', progress: event.progress})
}

async function forgetCachedVoices(config) {
    if (!config.backendHosted) {
        await dropBrokenVoiceCache(self.caches)
        return
    }
    try {
        await browserCaches?.delete?.(KOKORO_VOICE_CACHE)
    } catch (_error) {
        // Nothing to free.
    }
}

function load(config) {
    if (!ttsPromise) {
        configure(config)
        // Voice files an earlier build kept in Cache Storage are not read any more (backend) or are
        // checked, so a failed download stored as a voice is not read back every time (mirror).
        ttsPromise = forgetCachedVoices(config).then(() => KokoroTTS.from_pretrained(config.model, {
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
    if (type === 'cancel') {
        queue.cancelThrough(payload?.through)
        return
    }
    try {
        const tts = await load(payload.config)
        if (type === 'load') {
            self.postMessage({id, ok: true})
            return
        }
        const output = await queue.enqueue(id, () => tts.generate(payload.text, {voice: payload.config.voice}))
        if (!output) {
            self.postMessage({id, ok: false, cancelled: true})
            return
        }
        const samples = output.audio
        self.postMessage({id, ok: true, samples, sampleRate: output.sampling_rate}, [samples.buffer])
    } catch (error) {
        self.postMessage({id, ok: false, message: error?.message || String(error)})
    }
}
