import {defineConfig} from 'vite'
import react from '@vitejs/plugin-react'

export default defineConfig({
    base: './',
    plugins: [react()],
    resolve: {
        // ONNX Runtime's "extern wasm" entry points do not embed the Asyncify build's loader, so the
        // bundle carries only the binary the speech workers load (src/features/voice/local/ortWasm.js).
        conditions: ['onnxruntime-web-use-extern-wasm']
    },
    server: {
        port: 5173
    },
    build: {
        outDir: 'dist'
    }
})
