const {app, BrowserWindow, ipcMain} = require('electron')
const path = require('path')
const fs = require('fs')
const {spawn} = require('child_process')
const keytar = require('keytar')
const WebSocket = require('ws')

const SERVICE = 'OMyPIc'
let backendProcess = null
let mongoProcess = null
const realtimeSockets = new Map()
let realtimeSocketSeq = 0

function resolveResource(relativePath) {
    const base = app.isPackaged ? process.resourcesPath : path.join(__dirname, '..')
    return path.join(base, relativePath)
}

function startBackend() {
    const customCmd = process.env.OMYPIC_BACKEND_CMD
    if (customCmd) {
        backendProcess = spawn(customCmd, {shell: true, stdio: 'inherit'})
        return
    }
    const jarPath = resolveResource('backend/omypic-backend.jar')
    if (fs.existsSync(jarPath)) {
        backendProcess = spawn('java', ['-jar', jarPath], {stdio: 'inherit'})
    }
}

function startMongo() {
    const customBin = process.env.OMYPIC_MONGODB_BIN
    const mongoBin = customBin || resolveResource('mongodb/bin/mongod')
    if (!fs.existsSync(mongoBin)) {
        return
    }
    const dataDir = path.join(app.getPath('userData'), 'db')
    fs.mkdirSync(dataDir, {recursive: true})
    mongoProcess = spawn(mongoBin, ['--dbpath', dataDir, '--bind_ip', '127.0.0.1', '--port', '27017'], {
        stdio: 'ignore'
    })
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

app.whenReady().then(() => {
    startMongo()
    startBackend()
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
    if (backendProcess) backendProcess.kill()
    if (mongoProcess) mongoProcess.kill()
})

ipcMain.handle('get-backend-url', () => {
    return process.env.OMYPIC_BACKEND_URL || 'http://localhost:4317'
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
    if (!sessionId || !apiKey) {
        throw new Error('sessionId and apiKey are required')
    }

    const socketId = `rt-${++realtimeSocketSeq}`
    const wsUrl = `${backendUrl.replace(/^http/i, 'ws')}/api/realtime/voice?sessionId=${encodeURIComponent(sessionId)}`
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
            sendEvent('close', {code, reason: reason?.toString() || ''})
            realtimeSockets.delete(socketId)
            if (!resolved) {
                reject(new Error(`Socket closed before open: ${code}`))
            }
        })
    })
})

ipcMain.handle('realtime-send', async (event, payload) => {
    const socketId = payload?.socketId
    const message = payload?.message
    const socket = realtimeSockets.get(socketId)
    if (!socket || socket.readyState !== WebSocket.OPEN) {
        throw new Error('Realtime socket is not open')
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
