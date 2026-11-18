import {chmod, copyFile, mkdir} from 'node:fs/promises'
import {existsSync} from 'node:fs'
import path from 'node:path'

// The MongoDB archives ship far more in bin/ than the app launches: on Windows
// mongod.pdb and mongos.pdb alone are ~1.6 GB. Only these files are bundled.
const runtimeFilesByPlatform = {
    win32: {
        binary: 'mongod.exe',
        // mongod.exe needs the VC++ runtime; keep its installer next to it.
        optional: ['vc_redist.x64.exe']
    },
    darwin: {
        binary: 'mongod',
        optional: []
    }
}

export async function copyMongoRuntime(sourceBinDir, targetDir, platform = process.platform) {
    const runtime = runtimeFilesByPlatform[platform]
    if (!runtime) {
        throw new Error(`Unsupported mongo platform: ${platform}`)
    }

    const sourceBin = path.join(sourceBinDir, runtime.binary)
    if (!existsSync(sourceBin)) {
        throw new Error(`MongoDB binary not found in archive: ${sourceBin}`)
    }

    await mkdir(targetDir, {recursive: true})
    const targetBin = path.join(targetDir, runtime.binary)
    await copyFile(sourceBin, targetBin)
    if (platform !== 'win32') {
        await chmod(targetBin, 0o755)
    }

    for (const name of runtime.optional) {
        const source = path.join(sourceBinDir, name)
        if (existsSync(source)) {
            await copyFile(source, path.join(targetDir, name))
        }
    }

    return targetBin
}
