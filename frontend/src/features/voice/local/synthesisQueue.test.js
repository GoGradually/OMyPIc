import {describe, expect, it} from 'vitest'
import {createSynthesisQueue} from './synthesisQueue.js'

describe('synthesis queue', () => {
    it('reads a cancel that arrived while a synthesis kept the thread busy before starting the next one', async () => {
        const queue = createSynthesisQueue()
        const started = []
        const synthesize = (label) => () => {
            started.push(label)
            if (label === 'first') {
                // The model runs on the worker thread: a cancel message arrives meanwhile and waits
                // for its turn, as a task, while the rest of the queue moves on through promise callbacks.
                setTimeout(() => queue.cancelThrough(3), 0)
            }
            return Promise.resolve(label)
        }

        const results = await Promise.all([
            queue.enqueue(1, synthesize('first')),
            queue.enqueue(2, synthesize('second')),
            queue.enqueue(3, synthesize('third')),
            queue.enqueue(4, synthesize('after the cancel'))
        ])

        expect(started).toEqual(['first', 'after the cancel'])
        expect(results).toEqual(['first', null, null, 'after the cancel'])
    })

    it('keeps going after a failed synthesis', async () => {
        const queue = createSynthesisQueue()

        const failed = queue.enqueue(1, () => Promise.reject(new Error('out of memory')))
        const next = queue.enqueue(2, () => Promise.resolve('next'))

        await expect(failed).rejects.toThrow('out of memory')
        await expect(next).resolves.toBe('next')
    })
})
