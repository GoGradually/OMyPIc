const {contextBridge, ipcRenderer} = require('electron')

contextBridge.exposeInMainWorld('omypic', {
    getBackendUrl: () => ipcRenderer.invoke('get-backend-url'),
    getApiKey: () => ipcRenderer.invoke('get-api-key'),
    setApiKey: (key) => ipcRenderer.invoke('set-api-key', key),
    saveBackupFile: (defaultName, bytes) => ipcRenderer.invoke('save-backup-file', defaultName, bytes),
    pickBackupFile: () => ipcRenderer.invoke('pick-backup-file')
})
