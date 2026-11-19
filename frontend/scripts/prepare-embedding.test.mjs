import {createHash} from 'node:crypto'
import {mkdtemp, readdir, readFile, rm, writeFile, mkdir} from 'node:fs/promises'
import {tmpdir} from 'node:os'
import path from 'node:path'
import {afterEach, beforeEach, describe, expect, it} from 'vitest'
import {fileSha256, prepareEmbedding} from './prepare-embedding.mjs'

const MODEL_BYTES = 'fake gguf model bytes'
const MODEL_SHA = createHash('sha256').update(MODEL_BYTES).digest('hex')

describe('prepareEmbedding', () => {
    let root
    let resourcesRoot
    let modelPath

    beforeEach(async () => {
        root = await mkdtemp(path.join(tmpdir(), 'omypic-embedding-'))
        resourcesRoot = path.join(root, 'resources')
        modelPath = path.join(root, 'models', 'bge-m3.gguf')
        await mkdir(path.dirname(modelPath), {recursive: true})
        await writeFile(modelPath, MODEL_BYTES)
    })

    afterEach(async () => {
        await rm(root, {recursive: true, force: true})
    })

    const run = (env) => prepareEmbedding({env, resourcesRoot, projectRoot: root, log: () => {}})

    it('computes the file sha256', async () => {
        expect(await fileSha256(modelPath)).toBe(MODEL_SHA)
    })

    it('leaves only .gitkeep without the env flag', async () => {
        await mkdir(path.join(resourcesRoot, 'models'), {recursive: true})
        await writeFile(path.join(resourcesRoot, 'models', 'old.gguf'), 'stale')

        expect(await run({OMYPIC_EMBEDDING_MODEL_SOURCE: modelPath})).toBeNull()

        expect(await readdir(resourcesRoot)).toEqual(['.gitkeep'])
        expect(await readFile(path.join(resourcesRoot, '.gitkeep'), 'utf8')).toBe('')
    })

    it('copies the model and writes the manifest', async () => {
        const manifest = await run({
            OMYPIC_BUNDLE_EMBEDDING: '1',
            OMYPIC_EMBEDDING_MODEL_SOURCE: 'models/bge-m3.gguf',
            OMYPIC_EMBEDDING_MODEL_SHA256: ` ${MODEL_SHA.toUpperCase()} `
        })

        expect(manifest).toEqual({filename: 'bge-m3.gguf', sha256: MODEL_SHA})
        const modelsDir = path.join(resourcesRoot, 'models')
        expect((await readdir(modelsDir)).sort()).toEqual(['bge-m3.gguf', 'manifest.json'])
        expect(await readFile(path.join(modelsDir, 'bge-m3.gguf'), 'utf8')).toBe(MODEL_BYTES)
        expect(JSON.parse(await readFile(path.join(modelsDir, 'manifest.json'), 'utf8')))
            .toEqual({filename: 'bge-m3.gguf', sha256: MODEL_SHA})
    })

    it('names the copy after OMYPIC_EMBEDDING_MODEL_VERSION', async () => {
        const manifest = await run({
            OMYPIC_BUNDLE_EMBEDDING: '1',
            OMYPIC_EMBEDDING_MODEL_SOURCE: modelPath,
            OMYPIC_EMBEDDING_MODEL_VERSION: 'bge-m3-v2.gguf'
        })

        expect(manifest.filename).toBe('bge-m3-v2.gguf')
        expect(await readdir(path.join(resourcesRoot, 'models'))).toContain('bge-m3-v2.gguf')
    })

    it('rejects a model whose sha256 does not match', async () => {
        await expect(run({
            OMYPIC_BUNDLE_EMBEDDING: '1',
            OMYPIC_EMBEDDING_MODEL_SOURCE: modelPath,
            OMYPIC_EMBEDDING_MODEL_SHA256: '0'.repeat(64)
        })).rejects.toThrow('sha256 mismatch at source')
        expect(await readdir(resourcesRoot)).toEqual(['.gitkeep'])
    })

    it('requires a source when bundling', async () => {
        await expect(run({OMYPIC_BUNDLE_EMBEDDING: '1'})).rejects.toThrow('OMYPIC_EMBEDDING_MODEL_SOURCE is required')
    })

    it('rejects a missing source file', async () => {
        await expect(run({OMYPIC_BUNDLE_EMBEDDING: '1', OMYPIC_EMBEDDING_MODEL_SOURCE: 'nope.gguf'}))
            .rejects.toThrow('Embedding model source not found')
    })
})
