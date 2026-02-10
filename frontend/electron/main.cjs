const {app, BrowserWindow, dialog, ipcMain} = require('electron')
const path = require('path')
const fs = require('fs')
const {spawn} = require('child_process')
const net = require('net')
const http = require('http')
const https = require('https')
const keytar = require('keytar')
const WebSocket = require('ws')

const SERVICE = 'OMyPIc'
let backendProcess = null
let mongoProcess = null
const realtimeSockets = new Map()
let realtimeSocketSeq = 0
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
    fs.mkdirSync(dataDir, {recursive: true})

    backendProcess = spawn(javaCommand, ['-jar', jarPath], {
        stdio: 'inherit',
        env: {
            ...process.env,
            OMYPIC_MONGODB_URI: MONGO_URI,
            OMYPIC_DATA_DIR: dataDir
        }
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
    for (const socket of realtimeSockets.values()) {
        try {
            socket.close()
        } catch (e) {
            // ignore
        }
    }
    realtimeSockets.clear()
    stopProcess(backendProcess)
    stopProcess(mongoProcess)
})

ipcMain.handle('get-backend-url', () => {
    return BACKEND_URL
})

ipcMain.handle('get-api-key', async (event, provider) => {
    return keytar.getPassword(SERVICE, provider)
})

ipcMain.handle('set-api-key', async (event, provider, key) => {
    await keytar.setPassword(SERVICE, provider, key)
    return true
})

ipcMain.handle('delete-api-key', async (event, provider) => {
    await keytar.deletePassword(SERVICE, provider)
    return true
})

ipcMain.handle('realtime-connect', async (event, payload) => {
    const backendUrl = payload?.backendUrl || process.env.OMYPIC_BACKEND_URL || 'http://localhost:4317'
    const sessionId = payload?.sessionId
    const apiKey = payload?.apiKey
    const conversationModel = payload?.conversationModel
    const sttModel = payload?.sttModel
    if (!sessionId || !apiKey) {
        throw new Error('sessionId and apiKey are required')
    }

    const socketId = `rt-${++realtimeSocketSeq}`
    const params = new URLSearchParams({sessionId})
    if (conversationModel) {
        params.set('conversationModel', conversationModel)
    }
    if (sttModel) {
        params.set('sttModel', sttModel)
    }
    const wsUrl = `${backendUrl.replace(/^http/i, 'ws')}/api/realtime/voice?${params.toString()}`
    return await new Promise((resolve, reject) => {
        const socket = new WebSocket(wsUrl, {
            headers: {
                'X-API-Key': apiKey
            }
        })
        realtimeSockets.set(socketId, socket)

        let resolved = false
        const sendEvent = (type, data) => {
            if (!event.sender.isDestroyed()) {
                event.sender.send('realtime-event', {socketId, type, data})
            }
        }

        socket.on('open', () => {
            sendEvent('open', null)
            resolved = true
            resolve(socketId)
        })

        socket.on('message', (data) => sendEvent('message', data.toString()))

        socket.on('error', (error) => {
            const message = error?.message || 'Socket error'
            sendEvent('error', message)
            if (!resolved) {
                realtimeSockets.delete(socketId)
                reject(new Error(message))
            }
        })

        socket.on('close', (code, reason) => {
            const closeReason = reason?.toString() || ''
            sendEvent('close', {code, reason: closeReason})
            realtimeSockets.delete(socketId)
            if (!resolved) {
                const suffix = closeReason ? ` (${closeReason})` : ''
                reject(new Error(`Socket closed before open: ${code}${suffix}`))
            }
        })
    })
})

ipcMain.handle('realtime-send', async (event, payload) => {
    const socketId = payload?.socketId
    const message = payload?.message
    const socket = realtimeSockets.get(socketId)
    if (!socket || socket.readyState !== WebSocket.OPEN) {
        return false
    }
    socket.send(message)
    return true
})

ipcMain.handle('realtime-close', async (event, socketId) => {
    const socket = realtimeSockets.get(socketId)
    if (!socket) {
        return false
    }
    socket.close()
    realtimeSockets.delete(socketId)
    return true
})
