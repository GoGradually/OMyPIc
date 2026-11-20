export const OPENAI_PROVIDER = 'openai'
export const ANTHROPIC_PROVIDER = 'anthropic'
const FEEDBACK_API_KEY_HEADER = 'X-Feedback-API-Key'

export async function getBackendUrl() {
    if (window.omypic && window.omypic.getBackendUrl) {
        return await window.omypic.getBackendUrl()
    }
    return 'http://localhost:4317'
}

export async function getApiKey(provider = OPENAI_PROVIDER) {
    if (window.omypic && window.omypic.getApiKey) {
        return await window.omypic.getApiKey(provider)
    }
    return ''
}

export async function setApiKey(key, provider = OPENAI_PROVIDER) {
    if (window.omypic && window.omypic.setApiKey) {
        return await window.omypic.setApiKey(key, provider)
    }
    return null
}

export async function verifyApiKey(apiKey, model, provider = OPENAI_PROVIDER) {
    const baseUrl = await getBackendUrl()
    const response = await fetch(`${baseUrl}/api/keys/verify`, {
        method: 'POST',
        headers: {'Content-Type': 'application/json'},
        body: JSON.stringify({provider, apiKey, model})
    })
    if (!response.ok) {
        throw new Error(await readErrorMessage(response, 'API key verification failed'))
    }
    return response.json()
}

// The backend answers errors as {"message": "..."}; show that sentence rather than raw JSON.
async function readErrorMessage(response, fallback) {
    const text = await response.text()
    if (!text) {
        return fallback
    }
    try {
        const parsed = JSON.parse(text)
        return typeof parsed?.message === 'string' && parsed.message ? parsed.message : text
    } catch (_error) {
        return text
    }
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
        throw new Error(await readErrorMessage(response, 'Request failed'))
    }
    return response
}

export async function getModelMeta() {
    const response = await callApi('/api/meta/models')
    return response.json()
}

// The backup, plus how many rulebooks the backend left out because their file was missing (the
// response header is only sent when there is at least one).
export async function exportDataBundle() {
    const response = await callApi('/api/data-transfer/export')
    const missing = Number.parseInt(response.headers?.get?.('x-omypic-missing-rulebook-files') ?? '', 10)
    return {
        blob: await response.blob(),
        missingRulebookFiles: Number.isFinite(missing) && missing > 0 ? missing : 0
    }
}

export async function importDataBundle(file) {
    const formData = new FormData()
    formData.append('file', file)
    const response = await callApi('/api/data-transfer/import', {
        method: 'POST',
        body: formData
    })
    return response.json()
}

export async function openVoiceSession(payload) {
    const headers = {'Content-Type': 'application/json'}
    // With AI on, STT/TTS run on the OpenAI key (X-API-Key) and a Claude feedback model needs its own key.
    // With AI off no provider is called, so no key is required at all.
    const needsFeedbackKey = payload?.aiEnabled !== false && payload?.feedbackProvider === ANTHROPIC_PROVIDER
    if (needsFeedbackKey) {
        const feedbackApiKey = await getApiKey(ANTHROPIC_PROVIDER)
        if (feedbackApiKey) {
            headers[FEEDBACK_API_KEY_HEADER] = feedbackApiKey
        }
    }
    const response = await callApi('/api/voice/sessions', {
        method: 'POST',
        headers,
        body: JSON.stringify(payload)
    })
    return response.json()
}

export async function sendVoiceAudioChunk(voiceSessionId, payload, requestOptions = {}) {
    await callApi(`/api/voice/sessions/${encodeURIComponent(voiceSessionId)}/audio-chunks`, {
        method: 'POST',
        headers: {'Content-Type': 'application/json'},
        body: JSON.stringify(payload),
        ...requestOptions
    })
}

// AI off: only the end of the answer is reported. The recording never leaves the device.
export async function sendVoiceAnswer(voiceSessionId, payload, requestOptions = {}) {
    await callApi(`/api/voice/sessions/${encodeURIComponent(voiceSessionId)}/answers`, {
        method: 'POST',
        headers: {'Content-Type': 'application/json'},
        body: JSON.stringify(payload),
        ...requestOptions
    })
}

export async function stopVoiceSession(voiceSessionId, payload = {forced: true, reason: 'user_stop'}) {
    await callApi(`/api/voice/sessions/${encodeURIComponent(voiceSessionId)}/stop`, {
        method: 'POST',
        headers: {'Content-Type': 'application/json'},
        body: JSON.stringify(payload)
    })
}

export async function recoverVoiceSession(voiceSessionId, lastSeenEventId = 0) {
    const query = Number.isFinite(lastSeenEventId)
        ? `?lastSeenEventId=${encodeURIComponent(Math.max(0, Number(lastSeenEventId)))}`
        : ''
    const response = await callApi(`/api/voice/sessions/${encodeURIComponent(voiceSessionId)}/recovery${query}`)
    return response.json()
}

export async function getVoiceEventsUrl(voiceSessionId, sinceEventId = null) {
    const baseUrl = await getBackendUrl()
    const query = Number.isFinite(sinceEventId)
        ? `?sinceEventId=${encodeURIComponent(Math.max(0, Number(sinceEventId)))}`
        : ''
    return `${baseUrl}/api/voice/sessions/${encodeURIComponent(voiceSessionId)}/events${query}`
}
