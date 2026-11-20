/** @vitest-environment jsdom */
import {afterEach, beforeEach, describe, expect, it, vi} from 'vitest'
import {getBackendUrl} from '../../../shared/api/http.js'

vi.mock('../../../shared/api/http.js', () => ({
    getBackendUrl: vi.fn(async () => 'http://localhost:4317')
}))

// Answers every request with whatever `respond` returns for it.
let posted
let respond

function installWorker() {
    vi.stubGlobal('Worker', class {
        constructor(url, options) {
            this.url = String(url)
            this.options = options
        }

        postMessage(message) {
            posted.push(message)
            queueMicrotask(() => this.onmessage({data: respond(message)}))
        }

        terminate() {}
    })
}

describe('local Korean tts', () => {
    beforeEach(() => {
        posted = []
        respond = (message) => ({id: message.id, ok: true})
        vi.resetModules()
        installWorker()
    })

    afterEach(() => {
        vi.unstubAllEnvs()
        vi.unstubAllGlobals()
    })

    it('loads the Korean model through the backend model cache by default', async () => {
        const {loadLocalKoTts} = await import('./localKoTts.js')

        await loadLocalKoTts()

        expect(posted).toHaveLength(1)
        expect(posted[0].type).toBe('load')
        expect(posted[0].payload.config).toEqual({
            model: 'onnx-community/Supertonic-TTS-2-ONNX',
            dtype: 'fp32',
            voice: 'F2',
            options: {num_inference_steps: 5, speed: 1.05},
            modelHost: 'http://localhost:4317/api/local-models/',
            modelPathTemplate: '',
            backendHosted: true,
            voiceUrl: 'http://localhost:4317/api/local-models/onnx-community/Supertonic-TTS-2-ONNX/resolve/main/voices/F2.bin'
        })
    })

    it('lets build variables override the model, voice, host and path template', async () => {
        vi.stubEnv('VITE_OMYPIC_LOCAL_KO_TTS_MODEL', 'mirror/ko-tts')
        vi.stubEnv('VITE_OMYPIC_LOCAL_KO_TTS_VOICE', 'M1')
        vi.stubEnv('VITE_OMYPIC_LOCAL_KO_TTS_HOST', 'https://mirror.example/models/')
        vi.stubEnv('VITE_OMYPIC_LOCAL_KO_TTS_PATH_TEMPLATE', '{model}/main/')
        const {loadLocalKoTts} = await import('./localKoTts.js')

        await loadLocalKoTts()

        expect(posted[0].payload.config).toMatchObject({
            model: 'mirror/ko-tts',
            voice: 'M1',
            modelHost: 'https://mirror.example/models/',
            modelPathTemplate: '{model}/main/',
            backendHosted: false,
            voiceUrl: 'https://mirror.example/models/mirror/ko-tts/main/voices/M1.bin'
        })
    })

    it('runs in its own worker, apart from the English one', async () => {
        const created = []
        vi.stubGlobal('Worker', class {
            constructor(url) {
                created.push(String(url))
            }

            postMessage(message) {
                queueMicrotask(() => this.onmessage({data: {id: message.id, ok: true}}))
            }
        })
        const {loadLocalKoTts} = await import('./localKoTts.js')

        await loadLocalKoTts()

        expect(created).toHaveLength(1)
        expect(created[0]).toContain('localKoTtsWorker.js')
    })

    it('returns the synthesized samples and their sample rate', async () => {
        const samples = new Float32Array([0.1, -0.2])
        respond = (message) => (message.type === 'synthesize'
            ? {id: message.id, ok: true, samples, sampleRate: 44100}
            : {id: message.id, ok: true})
        const {synthesizeKorean} = await import('./localKoTts.js')

        const audio = await synthesizeKorean('전반적으로 내용 전달은 좋았습니다.')

        expect(audio).toEqual({samples, sampleRate: 44100})
        expect(posted.map((message) => message.type)).toEqual(['load', 'synthesize'])
        expect(posted[1].payload.text).toBe('전반적으로 내용 전달은 좋았습니다.')
    })

    it('builds the model settings once per load, not once per sentence', async () => {
        respond = (message) => (message.type === 'synthesize'
            ? {id: message.id, ok: true, samples: new Float32Array([0.1]), sampleRate: 44100}
            : {id: message.id, ok: true})
        const {synthesizeKorean} = await import('./localKoTts.js')
        getBackendUrl.mockClear()

        await synthesizeKorean('첫 번째 문장입니다.')
        await synthesizeKorean('두 번째 문장입니다.')

        expect(getBackendUrl).toHaveBeenCalledTimes(1)
        expect(posted.map((message) => message.type)).toEqual(['load', 'synthesize', 'synthesize'])
        expect(posted[1].payload.config).toEqual(posted[0].payload.config)
        expect(posted[2].payload.config).toEqual(posted[0].payload.config)
    })

    it('rejects with the worker error and loads again on the next call', async () => {
        respond = (message) => ({id: message.id, ok: false, message: 'HTTP 403 for config.json'})
        const {getLocalKoTtsStatus, loadLocalKoTts} = await import('./localKoTts.js')

        await expect(loadLocalKoTts()).rejects.toThrow('HTTP 403 for config.json')
        expect(getLocalKoTtsStatus()).toBe('failed')

        respond = (message) => ({id: message.id, ok: true})
        await loadLocalKoTts()

        expect(posted).toHaveLength(2)
        expect(getLocalKoTtsStatus()).toBe('ready')
    })

    it('is no longer ready after its worker crashes', async () => {
        let instance
        vi.stubGlobal('Worker', class {
            constructor() {
                instance = this
            }

            postMessage(message) {
                posted.push(message)
                queueMicrotask(() => this.onmessage({data: {id: message.id, ok: true}}))
            }

            terminate() {}
        })
        const {getLocalKoTtsStatus, loadLocalKoTts} = await import('./localKoTts.js')
        await loadLocalKoTts()
        expect(getLocalKoTtsStatus()).toBe('ready')

        instance.onerror({message: 'out of memory'})

        expect(getLocalKoTtsStatus()).toBe('failed')
    })

    it('reports its state while it loads and shares one load between callers', async () => {
        let answer
        respond = (message) => {
            answer = message
            return undefined
        }
        let instance
        vi.stubGlobal('Worker', class {
            constructor() {
                instance = this
            }

            postMessage(message) {
                posted.push(message)
                respond(message)
            }
        })
        const {getLocalKoTtsStatus, loadLocalKoTts} = await import('./localKoTts.js')
        expect(getLocalKoTtsStatus()).toBe('idle')

        const first = loadLocalKoTts()
        await vi.waitFor(() => expect(posted).toHaveLength(1))
        expect(getLocalKoTtsStatus()).toBe('loading')
        const second = loadLocalKoTts()
        instance.onmessage({data: {id: answer.id, ok: true}})
        await Promise.all([first, second])

        expect(posted).toHaveLength(1)
        expect(getLocalKoTtsStatus()).toBe('ready')
    })

    it('passes download progress to the listener', async () => {
        respond = null
        let instance
        vi.stubGlobal('Worker', class {
            constructor() {
                instance = this
            }

            postMessage(message) {
                posted.push(message)
                queueMicrotask(() => {
                    this.onmessage({data: {type: 'progress', file: 'onnx-community/Supertonic-TTS-2-ONNX', progress: 40}})
                    this.onmessage({data: {id: message.id, ok: true}})
                })
            }
        })
        const {loadLocalKoTts} = await import('./localKoTts.js')
        const onProgress = vi.fn()

        await loadLocalKoTts(onProgress)

        expect(instance).toBeTruthy()
        expect(onProgress).toHaveBeenCalledWith(expect.objectContaining({progress: 40}))
    })

    it('asks the worker to drop the sentences it has not started, and settles them', async () => {
        const answers = []
        respond = (message) => {
            if (message.type === 'synthesize') {
                answers.push(message)
                return undefined
            }
            return message.type === 'load' ? {id: message.id, ok: true} : undefined
        }
        let instance
        vi.stubGlobal('Worker', class {
            constructor() {
                instance = this
            }

            postMessage(message) {
                posted.push(message)
                const reply = respond(message)
                if (reply) {
                    queueMicrotask(() => this.onmessage({data: reply}))
                }
            }
        })
        const {cancelQueuedKoreanSynthesis, synthesizeKorean} = await import('./localKoTts.js')
        // Nothing to cancel before the worker exists.
        cancelQueuedKoreanSynthesis()
        expect(posted).toHaveLength(0)

        const first = synthesizeKorean('첫 문장입니다.')
        const second = synthesizeKorean('둘째 문장입니다.')
        await vi.waitFor(() => expect(answers).toHaveLength(2))
        cancelQueuedKoreanSynthesis()

        expect(posted[posted.length - 1]).toEqual({type: 'cancel', payload: {through: answers[1].id}})
        instance.onmessage({data: {id: answers[0].id, ok: true, samples: new Float32Array([0.1]), sampleRate: 44100}})
        instance.onmessage({data: {id: answers[1].id, ok: false, cancelled: true}})
        await expect(first).resolves.toMatchObject({sampleRate: 44100})
        await expect(second).rejects.toMatchObject({cancelled: true})
    })
})
