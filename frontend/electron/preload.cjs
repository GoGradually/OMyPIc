const {contextBridge, ipcRenderer} = require('electron')

contextBridge.exposeInMainWorld('omypic', {
    getBackendUrl: () => ipcRenderer.invoke('get-backend-url'),
    getApiKey: (provider) => ipcRenderer.invoke('get-api-key', provider),
    setApiKey: (key, provider) => ipcRenderer.invoke('set-api-key', key, provider),
    saveBackupFile: (defaultName, bytes) => ipcRenderer.invoke('save-backup-file', defaultName, bytes),
    pickBackupFile: () => ipcRenderer.invoke('pick-backup-file')
})
