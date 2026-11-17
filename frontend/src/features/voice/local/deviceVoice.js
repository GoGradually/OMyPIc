// Speaks text with the voice built into the device (Web Speech synthesis).
// It calls no API and costs nothing; quality depends on the voices the OS provides.

const VOICE_WAIT_MS = 1500
const START_WATCHDOG_MS = 5000
const MAX_CHUNK_LENGTH = 220

export function isDeviceVoiceSupported() {
    return typeof window !== 'undefined'
        && typeof window.speechSynthesis !== 'undefined'
        && typeof window.SpeechSynthesisUtterance === 'function'
}

// Engines cut long utterances short, so text is spoken sentence by sentence.
export function splitForSpeech(text) {
    const chunks = []
    const sentences = String(text || '')
        .split(/\n+/)
        .flatMap((line) => line.match(/[^.!?。！？]+[.!?。！？]*/g) || [])
        .map((sentence) => sentence.trim())
        .filter(Boolean)
    for (const sentence of sentences) {
        let rest = sentence
        while (rest.length > MAX_CHUNK_LENGTH) {
            const cut = rest.lastIndexOf(' ', MAX_CHUNK_LENGTH)
            const end = cut > 0 ? cut : MAX_CHUNK_LENGTH
            chunks.push(rest.slice(0, end).trim())
            rest = rest.slice(end).trim()
        }
        if (rest) {
            chunks.push(rest)
        }
    }
    return chunks
}

// Questions are English; feedback may be Korean. Each sentence gets a voice for its own language.
export function detectSpeechLang(text) {
    return /[ㄱ-ㆎ가-힣]/.test(text) ? 'ko-KR' : 'en-US'
}

export function pickVoice(voices, lang) {
    const language = lang.slice(0, 2).toLowerCase()
    const candidates = voices.filter((voice) => (voice.lang || '').toLowerCase().startsWith(language))
    return candidates.find((voice) => (voice.lang || '').toLowerCase() === lang.toLowerCase() && voice.localService)
        || candidates.find((voice) => voice.localService)
        || candidates[0]
        || null
}

function loadVoices() {
    const synth = window.speechSynthesis
    const ready = synth.getVoices()
    if (ready.length > 0) {
        return Promise.resolve(ready)
    }
    // Voices load asynchronously on first use.
    return new Promise((resolve) => {
        let settled = false
        const finish = () => {
            if (settled) {
                return
            }
            settled = true
            synth.removeEventListener?.('voiceschanged', finish)
            resolve(synth.getVoices())
        }
        synth.addEventListener?.('voiceschanged', finish)
        setTimeout(finish, VOICE_WAIT_MS)
    })
}

/**
 * Speaks the text and calls onEnd when it finished, or onUnavailable when this device cannot
 * speak it (no speech engine, no voice, or the engine never started).
 * Returns a function that cancels the speech without calling either callback.
 */
export function speakWithDeviceVoice(text, {onEnd, onUnavailable} = {}) {
    let cancelled = false
    let watchdog = null
    const settle = (callback) => {
        if (cancelled) {
            return
        }
        cancelled = true
        clearTimeout(watchdog)
        callback?.()
    }
    const cancel = () => {
        const wasActive = !cancelled
        cancelled = true
        clearTimeout(watchdog)
        if (wasActive && isDeviceVoiceSupported()) {
            window.speechSynthesis.cancel()
        }
    }

    if (!isDeviceVoiceSupported()) {
        Promise.resolve().then(() => settle(onUnavailable))
        return cancel
    }

    loadVoices().then((voices) => {
        const chunks = splitForSpeech(text)
        if (cancelled) {
            return
        }
        if (voices.length === 0 || chunks.length === 0) {
            settle(chunks.length === 0 ? onEnd : onUnavailable)
            return
        }
        const speakChunk = (index) => {
            if (cancelled) {
                return
            }
            if (index >= chunks.length) {
                settle(onEnd)
                return
            }
            const lang = detectSpeechLang(chunks[index])
            const utterance = new window.SpeechSynthesisUtterance(chunks[index])
            const voice = pickVoice(voices, lang)
            utterance.lang = voice?.lang || lang
            if (voice) {
                utterance.voice = voice
            }
            utterance.onstart = () => clearTimeout(watchdog)
            utterance.onend = () => speakChunk(index + 1)
            utterance.onerror = () => {
                // The first sentence failing means nothing was heard; later failures just end early.
                settle(index === 0 ? onUnavailable : onEnd)
            }
            clearTimeout(watchdog)
            watchdog = setTimeout(() => {
                window.speechSynthesis.cancel()
                settle(index === 0 ? onUnavailable : onEnd)
            }, START_WATCHDOG_MS)
            window.speechSynthesis.speak(utterance)
        }
        window.speechSynthesis.cancel()
        speakChunk(0)
    })
    return cancel
}
