// Prepares text for the on-device speech models: sentence by sentence, each in its own language.

const MAX_CHUNK_LENGTH = 220

// Models cut long input short (the Korean model drops what is past its input limit), so text is
// spoken sentence by sentence, and a very long sentence in chunks of at most 220 characters.
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

// Questions are English; feedback may be Korean. A sentence with any Hangul is Korean, even when
// it quotes English, and is read whole by the Korean voice.
export function detectSpeechLang(text) {
    return /[ㄱ-ㆎ가-힣]/.test(text) ? 'ko-KR' : 'en-US'
}
