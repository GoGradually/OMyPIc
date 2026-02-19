const {app, BrowserWindow, dialog, ipcMain} = require('electron')
const path = require('path')
const fs = require('fs')
const {spawn} = require('child_process')
const net = require('net')
const http = require('http')
const https = require('https')
const keytar = require('keytar')

const SERVICE = 'OMyPIc'
const OPENAI_PROVIDER = 'openai'
let backendProcess = null
let mongoProcess = null
const BACKEND_URL = process.env.OMYPIC_BACKEND_URL || 'http://localhost:4317'
const MONGO_URI = process.env.OMYPIC_MONGODB_URI || 'mongodb://127.0.0.1:27017/omypic'

function resolveResource(relativePath) {
    const base = app.isPackaged ? process.resourcesPath : path.join(__dirname, '..')
    return path.join(base, relativePath)
}

function getBundledJavaPath() {
    const binary = process.platform === 'win32' ? 'java.exe' : 'java'
    return resolveResource(path.join('jre', 'bin', binary))
}

function getBundledMongoPath() {
    const binary = process.platform === 'win32' ? 'mongod.exe' : 'mongod'
    return resolveResource(path.join('mongodb', 'bin', binary))
}

function resolveBundledEmbeddingModel() {
    if (process.env.OMYPIC_RAG_MODEL_PATH) {
        return {
            path: process.env.OMYPIC_RAG_MODEL_PATH,
            version: process.env.OMYPIC_RAG_MODEL_VERSION || null,
            sha256: process.env.OMYPIC_RAG_MODEL_SHA256 || null
        }
    }
    const modelsDir = resolveResource('models')
    if (!fs.existsSync(modelsDir)) {
        return null
    }
    const entries = fs.readdirSync(modelsDir, {withFileTypes: true})
        .filter((entry) => entry.isFile() && entry.name !== 'manifest.json')
        .map((entry) => entry.name)
        .sort()
    if (entries.length === 0) {
        return null
    }
    const selected = entries[0]
    const manifestPath = path.join(modelsDir, 'manifest.json')
    let manifest = {}
    if (fs.existsSync(manifestPath)) {
        try {
            manifest = JSON.parse(fs.readFileSync(manifestPath, 'utf8'))
        } catch (_error) {
            manifest = {}
        }
    }
    return {
        path: path.join(modelsDir, selected),
        version: manifest.filename || selected,
        sha256: manifest.sha256 || null
    }
}

function waitForTcpPort(port, host = '127.0.0.1', timeoutMs = 30000, intervalMs = 300) {
    const deadline = Date.now() + timeoutMs
    return new Promise((resolve, reject) => {
        let settled = false
        const tryConnect = () => {
            if (settled) {
                return
            }
            const socket = new net.Socket()
            socket.setTimeout(intervalMs)
            socket.once('connect', () => {
                settled = true
                socket.destroy()
                resolve()
            })
            socket.once('timeout', () => socket.destroy())
            socket.once('error', () => socket.destroy())
            socket.once('close', () => {
                if (settled) {
                    return
                }
                if (Date.now() >= deadline) {
                    settled = true
                    reject(new Error(`Timed out waiting for ${host}:${port}`))
                    return
                }
                setTimeout(tryConnect, intervalMs)
            })
            socket.connect(port, host)
        }
        tryConnect()
    })
}

function httpGet(url) {
    const client = url.startsWith('https://') ? https : http
    return new Promise((resolve, reject) => {
        const req = client.get(url, (res) => {
            const chunks = []
            res.on('data', (chunk) => chunks.push(chunk))
            res.on('end', () => resolve({statusCode: res.statusCode, body: Buffer.concat(chunks).toString()}))
        })
        req.on('error', reject)
        req.setTimeout(5000, () => req.destroy(new Error('Health check timeout')))
    })
}

function waitForProcessUnexpectedExit(proc, name) {
    return new Promise((_, reject) => {
        proc.once('exit', (code, signal) => {
            reject(new Error(`${name} exited early (code=${code}, signal=${signal})`))
        })
    })
}

async function waitForHttpReady(url, timeoutMs = 60000, intervalMs = 500) {
    const deadline = Date.now() + timeoutMs
    while (Date.now() < deadline) {
        try {
            const {statusCode} = await httpGet(url)
            if (statusCode && statusCode >= 200 && statusCode < 300) {
                return
            }
        } catch (_error) {
            // ignore until timeout
        }
        await new Promise((resolve) => setTimeout(resolve, intervalMs))
    }
    throw new Error(`Timed out waiting for HTTP readiness: ${url}`)
}

async function startBackend() {
    const useExternalBackend = Boolean(process.env.OMYPIC_BACKEND_URL) && !process.env.OMYPIC_BACKEND_CMD
    if (useExternalBackend) {
        await waitForHttpReady(`${BACKEND_URL}/actuator/health`)
        return
    }

    const customCmd = process.env.OMYPIC_BACKEND_CMD
    if (customCmd) {
        backendProcess = spawn(customCmd, {shell: true, stdio: 'inherit'})
        await waitForHttpReady(`${BACKEND_URL}/actuator/health`)
        return
    }

    const jarPath = resolveResource('backend/omypic-backend.jar')
    if (!fs.existsSync(jarPath)) {
        if (!app.isPackaged) {
            console.log('[omypic] backend jar not found in dev mode; assuming external backend')
            return
        }
        throw new Error(`Backend jar not found: ${jarPath}`)
    }

    const bundledJavaPath = getBundledJavaPath()
    const javaCommand = fs.existsSync(bundledJavaPath) ? bundledJavaPath : 'java'
    const dataDir = process.env.OMYPIC_DATA_DIR || path.join(app.getPath('userData'), 'omypic-data')
    const bundledModel = resolveBundledEmbeddingModel()
    fs.mkdirSync(dataDir, {recursive: true})

    const backendEnv = {
        ...process.env,
        OMYPIC_MONGODB_URI: MONGO_URI,
        OMYPIC_DATA_DIR: dataDir
    }
    if (bundledModel) {
        backendEnv.OMYPIC_RAG_MODEL_PATH = bundledModel.path
        if (!backendEnv.OMYPIC_RAG_MODEL_VERSION && bundledModel.version) {
            backendEnv.OMYPIC_RAG_MODEL_VERSION = bundledModel.version
        }
        if (!backendEnv.OMYPIC_RAG_MODEL_SHA256 && bundledModel.sha256) {
            backendEnv.OMYPIC_RAG_MODEL_SHA256 = bundledModel.sha256
        }
    }

    backendProcess = spawn(javaCommand, ['-jar', jarPath], {
        stdio: 'inherit',
        env: backendEnv
    })
    backendProcess.on('error', (error) => {
        console.error('[omypic] backend process error', error)
    })

    await Promise.race([
        waitForHttpReady(`${BACKEND_URL}/actuator/health`),
        waitForProcessUnexpectedExit(backendProcess, 'Backend')
    ])
}

async function startMongo() {
    const useExternalMongo = Boolean(process.env.OMYPIC_MONGODB_URI) && !process.env.OMYPIC_MONGODB_BIN
    if (useExternalMongo) {
        return
    }

    const customBin = process.env.OMYPIC_MONGODB_BIN
    const mongoBin = customBin || getBundledMongoPath()
    if (!fs.existsSync(mongoBin)) {
        if (!app.isPackaged) {
            console.log('[omypic] mongodb binary not found in dev mode; assuming external mongodb')
            return
        }
        throw new Error(`MongoDB binary not found: ${mongoBin}`)
    }

    const dataDir = path.join(app.getPath('userData'), 'db')
    fs.mkdirSync(dataDir, {recursive: true})
    mongoProcess = spawn(mongoBin, ['--dbpath', dataDir, '--bind_ip', '127.0.0.1', '--port', '27017'], {
        stdio: 'inherit',
        env: process.env
    })
    mongoProcess.on('error', (error) => {
        console.error('[omypic] mongo process error', error)
    })

    await Promise.race([
        waitForTcpPort(27017),
        waitForProcessUnexpectedExit(mongoProcess, 'MongoDB')
    ])
}

function stopProcess(proc) {
    if (!proc) {
        return
    }
    try {
        proc.kill()
    } catch (_error) {
        // ignore shutdown failures
    }
}

function createWindow() {
    const win = new BrowserWindow({
        width: 1200,
        height: 900,
        webPreferences: {
            preload: path.join(__dirname, 'preload.cjs'),
            contextIsolation: true,
            nodeIntegration: false
        }
    })

    if (app.isPackaged) {
        win.loadFile(path.join(__dirname, '..', 'dist', 'index.html'))
    } else {
        win.loadURL('http://localhost:5173')
    }
}

app.whenReady().then(async () => {
    try {
        await startMongo()
        await startBackend()
    } catch (error) {
        console.error('[omypic] startup error', error)
        dialog.showErrorBox('OMyPIc startup error', `${error.message}\n\n앱을 종료합니다.`)
        app.exit(1)
        return
    }
    createWindow()
})

app.on('window-all-closed', () => {
    if (process.platform !== 'darwin') {
        app.quit()
    }
})

app.on('before-quit', () => {
    stopProcess(backendProcess)
    stopProcess(mongoProcess)
})

ipcMain.handle('get-backend-url', () => {
    return BACKEND_URL
})

ipcMain.handle('get-api-key', async () => {
    return keytar.getPassword(SERVICE, OPENAI_PROVIDER)
})

ipcMain.handle('set-api-key', async (event, key) => {
    await keytar.setPassword(SERVICE, OPENAI_PROVIDER, key)
    return true
})

ipcMain.handle('save-backup-file', async (event, defaultName, bytes) => {
    const {canceled, filePath} = await dialog.showSaveDialog({
        defaultPath: defaultName || 'omypic-backup.zip',
        filters: [{name: 'Zip Archive', extensions: ['zip']}]
    })
    if (canceled || !filePath) {
        return false
    }
    fs.writeFileSync(filePath, Buffer.from(bytes))
    return true
})

ipcMain.handle('pick-backup-file', async () => {
    const {canceled, filePaths} = await dialog.showOpenDialog({
        properties: ['openFile'],
        filters: [{name: 'Zip Archive', extensions: ['zip']}]
    })
    if (canceled || !filePaths || filePaths.length === 0) {
        return null
    }
    const filePath = filePaths[0]
    const bytes = fs.readFileSync(filePath)
    return {
        name: path.basename(filePath),
        bytes: Uint8Array.from(bytes)
    }
})
