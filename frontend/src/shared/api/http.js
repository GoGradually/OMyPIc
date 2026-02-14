const OPENAI_PROVIDER = 'openai'

export async function getBackendUrl() {
    if (window.omypic && window.omypic.getBackendUrl) {
        return await window.omypic.getBackendUrl()
    }
    return 'http://localhost:4317'
}

export async function getApiKey() {
    if (window.omypic && window.omypic.getApiKey) {
        return await window.omypic.getApiKey()
    }
    return ''
}

export async function setApiKey(key) {
    if (window.omypic && window.omypic.setApiKey) {
        return await window.omypic.setApiKey(key)
    }
    return null
}

export async function verifyApiKey(apiKey, model) {
    const baseUrl = await getBackendUrl()
    const response = await fetch(`${baseUrl}/api/keys/verify`, {
        method: 'POST',
        headers: {'Content-Type': 'application/json'},
        body: JSON.stringify({provider: OPENAI_PROVIDER, apiKey, model})
    })
    if (!response.ok) {
        const text = await response.text()
        throw new Error(text || 'API key verification failed')
    }
    return response.json()
}

export async function callApi(path, options = {}) {
    const baseUrl = await getBackendUrl()
    const apiKey = await getApiKey()
    const headers = options.headers ? {...options.headers} : {}
    if (apiKey) {
        headers['X-API-Key'] = apiKey
    }
    const response = await fetch(`${baseUrl}${path}`, {
        ...options,
        headers
    })
    if (!response.ok) {
        const text = await response.text()
        throw new Error(text || 'Request failed')
    }
    return response
}

export async function connectRealtime(sessionId, apiKey, conversationModel, sttModel) {
    if (!window.omypic || !window.omypic.realtimeConnect) {
        throw new Error('Realtime is only available in desktop app')
    }
    const backendUrl = await getBackendUrl()
    return window.omypic.realtimeConnect({backendUrl, sessionId, apiKey, conversationModel, sttModel})
}

export async function sendRealtime(socketId, payload) {
    if (!window.omypic || !window.omypic.realtimeSend) {
        throw new Error('Realtime is only available in desktop app')
    }
    return window.omypic.realtimeSend(socketId, JSON.stringify(payload))
}

export async function closeRealtime(socketId) {
    if (!window.omypic || !window.omypic.realtimeClose) {
        return false
    }
    return window.omypic.realtimeClose(socketId)
}

export function subscribeRealtime(callback) {
    if (!window.omypic || !window.omypic.onRealtimeEvent) {
        return () => {
        }
    }
    return window.omypic.onRealtimeEvent(callback)
}
