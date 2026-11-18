import {afterEach, beforeEach, describe, expect, it} from 'vitest'
import {mkdir, mkdtemp, readdir, rm, stat, writeFile} from 'node:fs/promises'
import os from 'node:os'
import path from 'node:path'
import {copyMongoRuntime} from './mongo-runtime.mjs'

// bin/ listings of the real MongoDB 7.0.14 archives.
const windowsBin = ['Install-Compass.ps1', 'mongod.exe', 'mongod.pdb', 'mongos.exe', 'mongos.pdb', 'vc_redist.x64.exe']
const macBin = ['._install_compass', 'install_compass', 'mongod', 'mongos']

describe('copyMongoRuntime', () => {
    let root
    let sourceBinDir
    let targetDir

    async function seed(names) {
        await mkdir(sourceBinDir, {recursive: true})
        await Promise.all(names.map((name) => writeFile(path.join(sourceBinDir, name), name)))
    }

    beforeEach(async () => {
        root = await mkdtemp(path.join(os.tmpdir(), 'omypic-mongo-'))
        sourceBinDir = path.join(root, 'extracted', 'bin')
        targetDir = path.join(root, 'resources', 'mongodb', 'bin')
    })

    afterEach(async () => {
        await rm(root, {recursive: true, force: true})
    })

    it('bundles only mongod.exe and the VC++ installer on Windows', async () => {
        await seed(windowsBin)

        const targetBin = await copyMongoRuntime(sourceBinDir, targetDir, 'win32')

        expect(targetBin).toBe(path.join(targetDir, 'mongod.exe'))
        expect((await readdir(targetDir)).sort()).toEqual(['mongod.exe', 'vc_redist.x64.exe'])
    })

    it('bundles an executable mongod on macOS', async () => {
        await seed(macBin)

        const targetBin = await copyMongoRuntime(sourceBinDir, targetDir, 'darwin')

        expect(targetBin).toBe(path.join(targetDir, 'mongod'))
        expect(await readdir(targetDir)).toEqual(['mongod'])
        expect((await stat(targetBin)).mode & 0o111).toBe(0o111)
    })

    it('fails when the archive has no mongod binary', async () => {
        await seed(['mongos.exe'])

        await expect(copyMongoRuntime(sourceBinDir, targetDir, 'win32')).rejects.toThrow(/MongoDB binary not found/)
    })
})
