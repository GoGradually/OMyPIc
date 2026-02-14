#!/usr/bin/env node

import {createHash} from 'node:crypto'
import {chmod, copyFile, cp, mkdir, readdir, rm, stat, writeFile} from 'node:fs/promises'
import {accessSync, createReadStream, createWriteStream} from 'node:fs'
import path from 'node:path'
import {fileURLToPath} from 'node:url'
import {Readable} from 'node:stream'
import {pipeline} from 'node:stream/promises'
import {spawn} from 'node:child_process'
import extract from 'extract-zip'
import * as tar from 'tar'

const __filename = fileURLToPath(import.meta.url)
const __dirname = path.dirname(__filename)
const frontendRoot = path.resolve(__dirname, '..')
const projectRoot = path.resolve(frontendRoot, '..')
const backendRoot = path.join(projectRoot, 'backend')
const resourcesRoot = path.join(frontendRoot, 'electron', 'resources')
const cacheRoot = path.join(frontendRoot, '.cache', 'binaries')
const backendTarget = path.join(resourcesRoot, 'backend', 'omypic-backend.jar')

const MONGO_VERSION = '7.0.14'
const TEMURIN_VERSION = '17.0.13_11'
const TEMURIN_RELEASE_TAG = 'jdk-17.0.13%2B11'

const mongoByPlatform = {
    win32: {
        x64: {
            archiveName: `mongodb-windows-x86_64-${MONGO_VERSION}.zip`,
            url: `https://fastdl.mongodb.org/windows/mongodb-windows-x86_64-${MONGO_VERSION}.zip`,
            shaUrl: `https://fastdl.mongodb.org/windows/mongodb-windows-x86_64-${MONGO_VERSION}.zip.sha256`
        }
    },
    darwin: {
        x64: {
            archiveName: `mongodb-macos-x86_64-${MONGO_VERSION}.tgz`,
            url: `https://fastdl.mongodb.org/osx/mongodb-macos-x86_64-${MONGO_VERSION}.tgz`,
            shaUrl: `https://fastdl.mongodb.org/osx/mongodb-macos-x86_64-${MONGO_VERSION}.tgz.sha256`
        },
        arm64: {
            archiveName: `mongodb-macos-arm64-${MONGO_VERSION}.tgz`,
            url: `https://fastdl.mongodb.org/osx/mongodb-macos-arm64-${MONGO_VERSION}.tgz`,
            shaUrl: `https://fastdl.mongodb.org/osx/mongodb-macos-arm64-${MONGO_VERSION}.tgz.sha256`
        }
    }
}

const jreByPlatform = {
    win32: {
        x64: {
            archiveName: `OpenJDK17U-jre_x64_windows_hotspot_${TEMURIN_VERSION}.zip`,
            url: `https://github.com/adoptium/temurin17-binaries/releases/download/${TEMURIN_RELEASE_TAG}/OpenJDK17U-jre_x64_windows_hotspot_${TEMURIN_VERSION}.zip`,
            shaUrl: `https://github.com/adoptium/temurin17-binaries/releases/download/${TEMURIN_RELEASE_TAG}/OpenJDK17U-jre_x64_windows_hotspot_${TEMURIN_VERSION}.zip.sha256.txt`
        }
    },
    darwin: {
        x64: {
            archiveName: `OpenJDK17U-jre_x64_mac_hotspot_${TEMURIN_VERSION}.tar.gz`,
            url: `https://github.com/adoptium/temurin17-binaries/releases/download/${TEMURIN_RELEASE_TAG}/OpenJDK17U-jre_x64_mac_hotspot_${TEMURIN_VERSION}.tar.gz`,
            shaUrl: `https://github.com/adoptium/temurin17-binaries/releases/download/${TEMURIN_RELEASE_TAG}/OpenJDK17U-jre_x64_mac_hotspot_${TEMURIN_VERSION}.tar.gz.sha256.txt`
        },
        arm64: {
            archiveName: `OpenJDK17U-jre_aarch64_mac_hotspot_${TEMURIN_VERSION}.tar.gz`,
            url: `https://github.com/adoptium/temurin17-binaries/releases/download/${TEMURIN_RELEASE_TAG}/OpenJDK17U-jre_aarch64_mac_hotspot_${TEMURIN_VERSION}.tar.gz`,
            shaUrl: `https://github.com/adoptium/temurin17-binaries/releases/download/${TEMURIN_RELEASE_TAG}/OpenJDK17U-jre_aarch64_mac_hotspot_${TEMURIN_VERSION}.tar.gz.sha256.txt`
        }
    }
}

function getVariant(map, name) {
    const platform = process.platform
    const arch = process.arch
    const variant = map[platform]?.[arch]
    if (!variant) {
        throw new Error(`Unsupported ${name} platform/arch: ${platform}/${arch}`)
    }
    return variant
}

async function runCommand(command, args, cwd) {
    await new Promise((resolve, reject) => {
        const child = spawn(command, args, {
            cwd,
            stdio: 'inherit',
            shell: false
        })

        child.on('error', reject)
        child.on('exit', (code) => {
            if (code === 0) {
                resolve()
                return
            }
            reject(new Error(`Command failed (${code}): ${command} ${args.join(' ')}`))
        })
    })
}

async function ensureEmptyDir(dir) {
    await rm(dir, {recursive: true, force: true})
    await mkdir(dir, {recursive: true})
}

async function findSpringBootJar() {
    const libsDir = path.join(backendRoot, 'bootstrap', 'build', 'libs')
    const entries = await readdir(libsDir)
    const jarFiles = entries
        .filter((name) => name.endsWith('.jar') && !name.endsWith('-plain.jar'))
        .map((name) => path.join(libsDir, name))

    if (jarFiles.length === 0) {
        throw new Error(`No Spring Boot jar found in ${libsDir}`)
    }

    const stats = await Promise.all(jarFiles.map(async (file) => ({file, info: await stat(file)})))
    stats.sort((a, b) => b.info.mtimeMs - a.info.mtimeMs)
    return stats[0].file
}

async function copyBackendJar() {
    console.log('[prepare] building backend bootJar')
    if (process.platform === 'win32') {
        await runCommand('cmd', ['/c', 'gradlew.bat', ':bootstrap:bootJar', '--no-daemon'], backendRoot)
    } else {
        await runCommand('./gradlew', [':bootstrap:bootJar', '--no-daemon'], backendRoot)
    }

    const sourceJar = await findSpringBootJar()
    await mkdir(path.dirname(backendTarget), {recursive: true})
    await copyFile(sourceJar, backendTarget)
    console.log(`[prepare] backend jar copied: ${backendTarget}`)
}

async function fileSha256(filePath) {
    const hash = createHash('sha256')
    await new Promise((resolve, reject) => {
        const stream = createReadStream(filePath)
        stream.on('data', (chunk) => hash.update(chunk))
        stream.on('error', reject)
        stream.on('end', resolve)
    })
    return hash.digest('hex')
}

async function fetchText(url) {
    const response = await fetch(url, {redirect: 'follow'})
    if (!response.ok) {
        throw new Error(`Failed to fetch ${url}: ${response.status}`)
    }
    return response.text()
}

async function fetchSha256(shaUrl) {
    const text = await fetchText(shaUrl)
    const match = text.match(/[a-fA-F0-9]{64}/)
    if (!match) {
        throw new Error(`Could not parse sha256 from ${shaUrl}`)
    }
    return match[0].toLowerCase()
}

async function downloadFile(url, destination) {
    const response = await fetch(url, {redirect: 'follow'})
    if (!response.ok || !response.body) {
        throw new Error(`Download failed ${url}: ${response.status}`)
    }
    await pipeline(Readable.fromWeb(response.body), createWriteStream(destination))
}

async function prepareArchive(artifact, category) {
    await mkdir(cacheRoot, {recursive: true})
    const archivePath = path.join(cacheRoot, artifact.archiveName)

    const expectedSha = await fetchSha256(artifact.shaUrl)
    let shouldDownload = true
    try {
        const currentSha = await fileSha256(archivePath)
        if (currentSha === expectedSha) {
            shouldDownload = false
        } else {
            console.log(`[prepare] ${category} cache hash mismatch, redownloading`)
        }
    } catch (_error) {
        shouldDownload = true
    }

    if (shouldDownload) {
        console.log(`[prepare] downloading ${category}: ${artifact.url}`)
        await downloadFile(artifact.url, archivePath)
        const downloadedSha = await fileSha256(archivePath)
        if (downloadedSha !== expectedSha) {
            throw new Error(`${category} sha256 mismatch: expected ${expectedSha}, actual ${downloadedSha}`)
        }
    } else {
        console.log(`[prepare] using cached ${category} archive`)
    }

    return archivePath
}

async function extractArchive(archivePath, destinationRoot) {
    await ensureEmptyDir(destinationRoot)
    if (archivePath.endsWith('.zip')) {
        await extract(archivePath, {dir: destinationRoot})
        return
    }
    if (archivePath.endsWith('.tgz') || archivePath.endsWith('.tar.gz')) {
        await tar.extract({
            file: archivePath,
            cwd: destinationRoot,
            strip: 0
        })
        return
    }
    throw new Error(`Unsupported archive type: ${archivePath}`)
}

async function findFirstEntry(dir) {
    const entries = await readdir(dir, {withFileTypes: true})
    if (entries.length === 0) {
        throw new Error(`Empty extraction directory: ${dir}`)
    }
    const dirs = entries.filter((entry) => entry.isDirectory())
    if (dirs.length === 0) {
        throw new Error(`No directory found in extraction root: ${dir}`)
    }
    const sorted = dirs.sort((a, b) => a.name.localeCompare(b.name))
    return path.join(dir, sorted[0].name)
}

function resolveJavaSourcePath(extractedRoot) {
    const macBundleRoot = path.join(extractedRoot, 'Contents', 'Home')
    if (process.platform !== 'win32' && pathExistsSync(macBundleRoot)) {
        return macBundleRoot
    }
    return extractedRoot
}

function pathExistsSync(targetPath) {
    try {
        accessSync(targetPath)
        return true
    } catch (_error) {
        return false
    }
}

async function copyMongoBinary() {
    const variant = getVariant(mongoByPlatform, 'mongo')
    const archivePath = await prepareArchive(variant, 'mongodb')
    const tempExtractRoot = path.join(cacheRoot, 'mongo-extracted')
    await extractArchive(archivePath, tempExtractRoot)

    const extractedRoot = await findFirstEntry(tempExtractRoot)
    const sourceBinDir = path.join(extractedRoot, 'bin')
    const targetDir = path.join(resourcesRoot, 'mongodb', 'bin')
    await cp(sourceBinDir, targetDir, {recursive: true, force: true})

    const targetBin = path.join(targetDir, process.platform === 'win32' ? 'mongod.exe' : 'mongod')
    if (process.platform !== 'win32') {
        await chmod(targetBin, 0o755)
    }
    console.log(`[prepare] mongodb binary copied: ${targetBin}`)
}

async function copyJreBinary() {
    const variant = getVariant(jreByPlatform, 'jre')
    const archivePath = await prepareArchive(variant, 'jre')
    const tempExtractRoot = path.join(cacheRoot, 'jre-extracted')
    await extractArchive(archivePath, tempExtractRoot)

    const extractedRoot = await findFirstEntry(tempExtractRoot)
    const sourceJreRoot = resolveJavaSourcePath(extractedRoot)
    const targetJreRoot = path.join(resourcesRoot, 'jre')
    await cp(sourceJreRoot, targetJreRoot, {recursive: true, force: true})

    const targetJava = path.join(targetJreRoot, 'bin', process.platform === 'win32' ? 'java.exe' : 'java')
    if (process.platform !== 'win32') {
        await chmod(targetJava, 0o755)
    }
    console.log(`[prepare] jre binary copied: ${targetJava}`)
}

function normalizeSha256(value) {
    return (value || '').trim().toLowerCase()
}

async function copyEmbeddingModel() {
    const bundleEmbedding = process.env.OMYPIC_BUNDLE_EMBEDDING === '1'
    const targetDir = path.join(resourcesRoot, 'models')
    await rm(targetDir, {recursive: true, force: true})
    if (!bundleEmbedding) {
        console.log('[prepare] embedding model bundling disabled')
        return
    }

    const sourceInput = process.env.OMYPIC_EMBEDDING_MODEL_SOURCE
    if (!sourceInput) {
        throw new Error('OMYPIC_EMBEDDING_MODEL_SOURCE is required when OMYPIC_BUNDLE_EMBEDDING=1')
    }
    const sourcePath = path.isAbsolute(sourceInput) ? sourceInput : path.resolve(projectRoot, sourceInput)
    if (!pathExistsSync(sourcePath)) {
        throw new Error(`Embedding model source not found: ${sourcePath}`)
    }

    const sourceStat = await stat(sourcePath)
    if (!sourceStat.isFile()) {
        throw new Error(`Embedding model source must be a file: ${sourcePath}`)
    }

    const expectedSha = normalizeSha256(process.env.OMYPIC_EMBEDDING_MODEL_SHA256)
    const sourceSha = await fileSha256(sourcePath)
    if (expectedSha && expectedSha !== sourceSha) {
        throw new Error(`Embedding model sha256 mismatch at source. expected=${expectedSha} actual=${sourceSha}`)
    }

    const targetName = process.env.OMYPIC_EMBEDDING_MODEL_VERSION || path.basename(sourcePath)
    const targetPath = path.join(targetDir, targetName)
    await mkdir(targetDir, {recursive: true})
    await copyFile(sourcePath, targetPath)

    const targetSha = await fileSha256(targetPath)
    if (expectedSha && expectedSha !== targetSha) {
        throw new Error(`Embedding model sha256 mismatch after copy. expected=${expectedSha} actual=${targetSha}`)
    }

    const manifest = {
        filename: targetName,
        sha256: targetSha
    }
    await writeFile(path.join(targetDir, 'manifest.json'), `${JSON.stringify(manifest, null, 2)}\n`, 'utf8')
    console.log(`[prepare] embedding model copied: ${targetPath}`)
}

async function main() {
    await mkdir(resourcesRoot, {recursive: true})
    await rm(path.join(resourcesRoot, 'backend'), {recursive: true, force: true})
    await rm(path.join(resourcesRoot, 'mongodb'), {recursive: true, force: true})
    await rm(path.join(resourcesRoot, 'jre'), {recursive: true, force: true})
    await rm(path.join(resourcesRoot, 'models'), {recursive: true, force: true})
    await copyBackendJar()
    await copyMongoBinary()
    await copyJreBinary()
    await copyEmbeddingModel()
    console.log('[prepare] all resources are ready')
}

main().catch((error) => {
    console.error(error)
    process.exit(1)
})
