// The voice built into the device (Web Speech synthesis). It calls no API and costs nothing;
// which languages it can speak depends on the voices installed in the OS.

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

// Questions are English; feedback may be Korean. Each sentence is spoken in its own language.
export function detectSpeechLang(text) {
    return /[ㄱ-ㆎ가-힣]/.test(text) ? 'ko-KR' : 'en-US'
}

/**
 * Picks a voice of the sentence's language, or null when the device has none.
 * A voice of another language is never returned: a Korean voice reading an English question
 * produces Korean-accented English, which defeats the listening practice.
 */
export function pickVoice(voices, lang) {
    const language = lang.slice(0, 2).toLowerCase()
    const candidates = voices.filter((voice) => (voice.lang || '').toLowerCase().replace('_', '-').startsWith(language))
    const exact = candidates.filter((voice) => (voice.lang || '').toLowerCase().replace('_', '-') === lang.toLowerCase())
    return exact.find((voice) => voice.localService)
        || exact[0]
        || candidates.find((voice) => voice.localService)
        || candidates[0]
        || null
}

/** Voices installed on this device. Empty when there is no speech engine. */
export function loadDeviceVoices() {
    if (!isDeviceVoiceSupported()) {
        return Promise.resolve([])
    }
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
 * Speaks one sentence with the given voice. Resolves true when it was spoken to the end and
 * false when the engine failed or never started. cancel() stops it and resolves false.
 */
export function speakSentence(sentence, voice) {
    let finish = () => {
    }
    let watchdog = null
    const done = new Promise((resolve) => {
        let settled = false
        finish = (spoken) => {
            if (settled) {
                return
            }
            settled = true
            clearTimeout(watchdog)
            resolve(spoken)
        }
    })
    const utterance = new window.SpeechSynthesisUtterance(sentence)
    utterance.voice = voice
    utterance.lang = voice.lang
    utterance.onstart = () => clearTimeout(watchdog)
    utterance.onend = () => finish(true)
    utterance.onerror = () => finish(false)
    watchdog = setTimeout(() => {
        window.speechSynthesis.cancel()
        finish(false)
    }, START_WATCHDOG_MS)
    window.speechSynthesis.speak(utterance)
    return {
        done,
        cancel: () => {
            window.speechSynthesis.cancel()
            finish(false)
        }
    }
}
