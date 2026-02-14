const {contextBridge, ipcRenderer} = require('electron')

contextBridge.exposeInMainWorld('omypic', {
    getBackendUrl: () => ipcRenderer.invoke('get-backend-url'),
    getApiKey: () => ipcRenderer.invoke('get-api-key'),
    setApiKey: (key) => ipcRenderer.invoke('set-api-key', key),
    deleteApiKey: () => ipcRenderer.invoke('delete-api-key'),
    realtimeConnect: (payload) => ipcRenderer.invoke('realtime-connect', payload),
    realtimeSend: (socketId, message) => ipcRenderer.invoke('realtime-send', {socketId, message}),
    realtimeClose: (socketId) => ipcRenderer.invoke('realtime-close', socketId),
    onRealtimeEvent: (callback) => {
        const handler = (_event, data) => callback(data)
        ipcRenderer.on('realtime-event', handler)
        return () => ipcRenderer.removeListener('realtime-event', handler)
    }
})
