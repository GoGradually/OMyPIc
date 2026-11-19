#!/usr/bin/env node
// Puts the optional bundled embedding model into src-tauri/resources/models before `tauri build`.
// OMYPIC_BUNDLE_EMBEDDING=1 copies OMYPIC_EMBEDDING_MODEL_SOURCE there (checked against
// OMYPIC_EMBEDDING_MODEL_SHA256 when given) and writes manifest.json {filename, sha256}, which the app
// reads at startup. Without the flag the resources folder only holds an empty .gitkeep.

import {createHash} from 'node:crypto'
import {createReadStream, existsSync} from 'node:fs'
import {copyFile, mkdir, rm, stat, writeFile} from 'node:fs/promises'
import path from 'node:path'
import {fileURLToPath} from 'node:url'

const __dirname = path.dirname(fileURLToPath(import.meta.url))
const frontendRoot = path.resolve(__dirname, '..')

export const DEFAULT_PROJECT_ROOT = path.resolve(frontendRoot, '..')
export const DEFAULT_RESOURCES_ROOT = path.join(frontendRoot, 'src-tauri', 'resources')

export async function fileSha256(filePath) {
    const hash = createHash('sha256')
    for await (const chunk of createReadStream(filePath)) {
        hash.update(chunk)
    }
    return hash.digest('hex')
}

function normalizeSha256(value) {
    return (value || '').trim().toLowerCase()
}

/**
 * Returns the manifest written, or null when bundling is off.
 * `projectRoot` resolves a relative OMYPIC_EMBEDDING_MODEL_SOURCE.
 */
export async function prepareEmbedding({
    env = process.env,
    resourcesRoot = DEFAULT_RESOURCES_ROOT,
    projectRoot = DEFAULT_PROJECT_ROOT,
    log = console.log
} = {}) {
    const targetDir = path.join(resourcesRoot, 'models')
    await mkdir(resourcesRoot, {recursive: true})
    await writeFile(path.join(resourcesRoot, '.gitkeep'), '')
    await rm(targetDir, {recursive: true, force: true})
    if (env.OMYPIC_BUNDLE_EMBEDDING !== '1') {
        log('[prepare-embedding] embedding model bundling disabled')
        return null
    }

    const sourceInput = env.OMYPIC_EMBEDDING_MODEL_SOURCE
    if (!sourceInput) {
        throw new Error('OMYPIC_EMBEDDING_MODEL_SOURCE is required when OMYPIC_BUNDLE_EMBEDDING=1')
    }
    const sourcePath = path.isAbsolute(sourceInput) ? sourceInput : path.resolve(projectRoot, sourceInput)
    if (!existsSync(sourcePath)) {
        throw new Error(`Embedding model source not found: ${sourcePath}`)
    }
    if (!(await stat(sourcePath)).isFile()) {
        throw new Error(`Embedding model source must be a file: ${sourcePath}`)
    }

    const expectedSha = normalizeSha256(env.OMYPIC_EMBEDDING_MODEL_SHA256)
    const sourceSha = await fileSha256(sourcePath)
    if (expectedSha && expectedSha !== sourceSha) {
        throw new Error(`Embedding model sha256 mismatch at source. expected=${expectedSha} actual=${sourceSha}`)
    }

    const targetName = env.OMYPIC_EMBEDDING_MODEL_VERSION || path.basename(sourcePath)
    const targetPath = path.join(targetDir, targetName)
    await mkdir(targetDir, {recursive: true})
    await copyFile(sourcePath, targetPath)

    const targetSha = await fileSha256(targetPath)
    if (expectedSha && expectedSha !== targetSha) {
        throw new Error(`Embedding model sha256 mismatch after copy. expected=${expectedSha} actual=${targetSha}`)
    }

    const manifest = {filename: targetName, sha256: targetSha}
    await writeFile(path.join(targetDir, 'manifest.json'), `${JSON.stringify(manifest, null, 2)}\n`, 'utf8')
    log(`[prepare-embedding] embedding model copied: ${targetPath}`)
    return manifest
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
    prepareEmbedding().catch((error) => {
        console.error(error)
        process.exit(1)
    })
}
