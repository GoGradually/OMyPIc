const {contextBridge, ipcRenderer} = require('electron')

contextBridge.exposeInMainWorld('omypic', {
    getBackendUrl: () => ipcRenderer.invoke('get-backend-url'),
    getApiKey: (provider) => ipcRenderer.invoke('get-api-key', provider),
    setApiKey: (provider, key) => ipcRenderer.invoke('set-api-key', provider, key),
    deleteApiKey: (provider) => ipcRenderer.invoke('delete-api-key', provider),
    realtimeConnect: (payload) => ipcRenderer.invoke('realtime-connect', payload),
    realtimeSend: (socketId, message) => ipcRenderer.invoke('realtime-send', {socketId, message}),
    realtimeClose: (socketId) => ipcRenderer.invoke('realtime-close', socketId),
    onRealtimeEvent: (callback) => {
        const handler = (_event, data) => callback(data)
        ipcRenderer.on('realtime-event', handler)
        return () => ipcRenderer.removeListener('realtime-event', handler)
    }
})
