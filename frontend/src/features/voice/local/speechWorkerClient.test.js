/** @vitest-environment jsdom */
import {afterEach, beforeEach, describe, expect, it, vi} from 'vitest'
import {createSpeechWorkerClient} from './speechWorkerClient.js'

const TEXTS = {
    unsupported: 'unsupported',
    crashed: 'crashed',
    failed: 'failed',
    timedOut: 'timed out'
}

// A worker that answers a load at once and keeps every other request until the test answers it.
function fakeWorkers() {
    const workers = []
    class FakeWorker {
        constructor() {
            this.posted = []
            this.terminated = false
            workers.push(this)
        }

        postMessage(message) {
            this.posted.push(message)
            if (message.type === 'load' && !FakeWorker.holdLoad) {
                queueMicrotask(() => this.onmessage?.({data: {id: message.id, ok: true}}))
            }
        }

        answer(message, data = {ok: true}) {
            this.onmessage?.({data: {id: message.id, ...data}})
        }

        requests(type = 'synthesize') {
            return this.posted.filter((message) => message.type === type)
        }

        terminate() {
            this.terminated = true
        }
    }
    FakeWorker.holdLoad = false
    return {workers, FakeWorker}
}

function makeClient(FakeWorker) {
    return createSpeechWorkerClient({
        createWorker: () => new FakeWorker(),
        buildConfig: async () => ({model: 'm'}),
        texts: TEXTS
    })
}

describe('speech worker client', () => {
    let workers
    let FakeWorker

    beforeEach(() => {
        ({workers, FakeWorker} = fakeWorkers())
        vi.stubGlobal('Worker', FakeWorker)
    })

    afterEach(() => {
        vi.useRealTimers()
        vi.unstubAllGlobals()
    })

    describe('watchdog', () => {
        it('fails a request the worker never answers, terminates it, and starts a fresh worker for the next one', async () => {
            vi.useFakeTimers()
            const client = makeClient(FakeWorker)
            const stuck = client.run('synthesize', (config) => ({config, text: 'a'}), {timeoutMs: 1000})
            const settled = stuck.catch((error) => error)
            await vi.advanceTimersByTimeAsync(0)
            expect(workers[0].requests()).toHaveLength(1)

            await vi.advanceTimersByTimeAsync(999)
            expect(client.getStatus()).toBe('ready')
            await vi.advanceTimersByTimeAsync(1)

            expect((await settled).message).toBe('timed out')
            expect(workers[0].terminated).toBe(true)
            expect(client.getStatus()).toBe('failed')
            expect(client.isBusy()).toBe(false)

            const next = client.run('synthesize', (config) => ({config, text: 'b'}), {timeoutMs: 1000})
            await vi.advanceTimersByTimeAsync(0)
            expect(workers).toHaveLength(2)
            expect(workers[1].posted.map((message) => message.type)).toEqual(['load', 'synthesize'])
            workers[1].answer(workers[1].requests()[0], {ok: true, samples: 1})
            await expect(next).resolves.toMatchObject({samples: 1})
            expect(client.getStatus()).toBe('ready')
        })

        it('counts from the last answer, so a long queue of requests that each finish in time is not cut', async () => {
            vi.useFakeTimers()
            const client = makeClient(FakeWorker)
            const runs = [1, 2, 3].map((n) => client.run('synthesize', (config) => ({config, text: String(n)}), {timeoutMs: 1000}))
            await vi.advanceTimersByTimeAsync(0)
            const [one, two, three] = workers[0].requests()

            await vi.advanceTimersByTimeAsync(900)
            workers[0].answer(one)
            await vi.advanceTimersByTimeAsync(900)
            workers[0].answer(two)
            await vi.advanceTimersByTimeAsync(900)
            workers[0].answer(three)

            await expect(Promise.all(runs)).resolves.toHaveLength(3)
            expect(workers[0].terminated).toBe(false)
        })

        it('does not limit the load, which includes the first download', async () => {
            vi.useFakeTimers()
            FakeWorker.holdLoad = true
            const client = makeClient(FakeWorker)
            const load = client.load()
            await vi.advanceTimersByTimeAsync(60 * 60 * 1000)

            expect(client.getStatus()).toBe('loading')
            expect(workers[0].terminated).toBe(false)
            workers[0].answer(workers[0].requests('load')[0])
            await load
            expect(client.getStatus()).toBe('ready')
        })
    })

    describe('cancel', () => {
        it('drops a request that was asked for but not yet sent when the cancel comes', async () => {
            const client = makeClient(FakeWorker)
            await client.load()

            const early = client.run('synthesize', (config) => ({config, text: 'a'}), {timeoutMs: 1000})
            client.cancelQueued()
            const later = client.run('synthesize', (config) => ({config, text: 'b'}), {timeoutMs: 1000})

            await expect(early).rejects.toMatchObject({cancelled: true})
            await vi.waitFor(() => expect(workers[0].requests()).toHaveLength(1))
            expect(workers[0].requests()[0].payload.text).toBe('b')
            workers[0].answer(workers[0].requests()[0])
            await expect(later).resolves.toBeTruthy()
        })
    })

    describe('release', () => {
        it('terminates the worker of a loaded model, is idle, and loads again in a new worker when asked', async () => {
            const client = makeClient(FakeWorker)
            const seen = []
            client.watchStatus(({status}) => seen.push(status))
            await client.load()

            expect(client.release()).toBe(true)
            expect(workers[0].terminated).toBe(true)
            expect(client.hasWorker()).toBe(false)
            expect(client.getStatus()).toBe('idle')

            await client.load()
            expect(workers).toHaveLength(2)
            expect(client.getStatus()).toBe('ready')
            expect(seen).toEqual(['loading', 'ready', 'idle', 'loading', 'ready'])
        })

        it('does nothing while the model loads or a request is under way', async () => {
            FakeWorker.holdLoad = true
            const client = makeClient(FakeWorker)
            const load = client.load()
            await vi.waitFor(() => expect(workers).toHaveLength(1))
            expect(client.release()).toBe(false)
            workers[0].answer(workers[0].requests('load')[0])
            await load

            const request = client.run('synthesize', (config) => ({config, text: 'a'}), {timeoutMs: 1000})
            // Not even before it is posted.
            expect(client.release()).toBe(false)
            await vi.waitFor(() => expect(workers[0].requests()).toHaveLength(1))
            expect(client.release()).toBe(false)
            expect(workers[0].terminated).toBe(false)
            workers[0].answer(workers[0].requests()[0])
            await request

            expect(client.release()).toBe(true)
        })

        it('returns false when there is nothing to release', () => {
            const client = makeClient(FakeWorker)
            expect(client.release()).toBe(false)
            expect(client.getStatus()).toBe('idle')
        })
    })
})
