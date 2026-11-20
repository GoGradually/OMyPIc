// The ONNX Runtime binary every speech worker loads (speech recognizer, English and Korean
// synthesizers). It is bundled with the app instead of being fetched from a CDN on first run, and
// imported from the copy @huggingface/transformers itself depends on, so it always matches the
// runtime code inside that library. One module, so the three workers cannot load different builds.
//
// This is the plain build, not the Asyncify one the library picks by default. Asyncify is only
// needed for WebGPU, and the workers run on the CPU (device 'wasm'). WebKit (the macOS and Linux
// desktop webview) spends minutes and several GB of memory compiling the Asyncify build, which
// hangs or kills the page; the plain build loads in seconds and runs as fast in every engine.
import ortFactoryUrl from 'onnxruntime-web/ort-wasm-simd-threaded.mjs?url'
import ortWasmUrl from 'onnxruntime-web/ort-wasm-simd-threaded.wasm?url'

/** The value for env.backends.onnx.wasm.wasmPaths, as absolute URLs against the worker's address. */
export function ortWasmPaths(baseHref) {
    return {
        mjs: new URL(ortFactoryUrl, baseHref).href,
        wasm: new URL(ortWasmUrl, baseHref).href
    }
}
