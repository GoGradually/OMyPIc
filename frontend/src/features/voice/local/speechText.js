// Prepares text for the on-device speech models: sentence by sentence, each in its own language.

const MAX_CHUNK_LENGTH = 220
const TERMINATORS = '.!?。！？'
// What may stand between a terminator and the space that ends the sentence: `He said "Stop."`
const CLOSERS = '"\'”’)]」』'
// Never end a sentence: `Mr. Lee`, `e.g. however`.
const ALWAYS_CONTINUE = new Set(['mr', 'mrs', 'ms', 'dr', 'prof', 'vs', 'e.g', 'i.e'])
// End a sentence unless a lowercase word follows: `etc. and juice`, `the U.S. now`.
const ETC = 'etc'
const INITIALISM = /^(\p{L}\.)+\p{L}$/u
const HANGUL = /\p{Script=Hangul}/u
const LATIN = /\p{Script=Latin}/u
const LETTER = /\p{L}/u
const SPACE = /\s/u

function isAbbreviation(before, after) {
    const word = (before.split(/\s+/u).pop() || '').replace(/^[("'“‘[]+/u, '').toLowerCase()
    if (ALWAYS_CONTINUE.has(word)) {
        return true
    }
    if (word === ETC || INITIALISM.test(word)) {
        return /^\p{Ll}/u.test(after.trimStart())
    }
    return false
}

// Opening and closing marks of a quoted span. A single quote opens only at the start of a word and
// closes only where no Latin letter follows, so the apostrophe in `don't` is neither.
function quoteOpenedAt(line, index) {
    const char = line[index]
    if (char === '"') {
        return '"'
    }
    if (char === '“') {
        return '”'
    }
    if ((char === '\'' || char === '‘') && (index === 0 || /[\s([]/u.test(line[index - 1]))) {
        return char === '‘' ? '’' : '\''
    }
    return null
}

function quoteClosesAt(line, index, closer) {
    if (line[index] !== closer) {
        return false
    }
    return closer === '"' || closer === '”' || !LATIN.test(line[index + 1] || '')
}

/**
 * The sentences of one line, as [start, end] offsets. A sentence ends at a terminator followed by
 * whitespace or the end of the line, after optional closing quotes or brackets. With trackQuotes a
 * terminator inside a quoted span does not end it; returns null when a quote is never closed.
 */
function sentenceRanges(line, trackQuotes) {
    const ranges = []
    let start = 0
    let closer = null
    for (let index = 0; index < line.length; index += 1) {
        if (trackQuotes) {
            if (closer) {
                if (quoteClosesAt(line, index, closer)) {
                    closer = null
                }
                continue
            }
            closer = quoteOpenedAt(line, index)
            if (closer) {
                continue
            }
        }
        if (!TERMINATORS.includes(line[index])) {
            continue
        }
        let end = index + 1
        while (end < line.length && TERMINATORS.includes(line[end])) {
            end += 1
        }
        while (end < line.length && CLOSERS.includes(line[end])) {
            end += 1
        }
        if (end < line.length && !SPACE.test(line[end])) {
            // `3.5`, `$4.50`, `a.b?c=d`, `3:30`: not the end of a sentence.
            continue
        }
        if (line[index] === '.' && end === index + 1 && isAbbreviation(line.slice(start, index), line.slice(end))) {
            continue
        }
        ranges.push([start, end])
        start = end
        index = end - 1
    }
    if (closer) {
        return null
    }
    ranges.push([start, line.length])
    return ranges
}

/**
 * Splits one line into sentences. A chunk without letters (a list number `1.`, an emoji) joins the
 * sentence after it on the line, or else the one before it; alone on its line it stays by itself
 * and nobody reads it (detectSpeechLang gives null).
 */
function splitLine(line) {
    // Spec 5.7: a Korean sentence that quotes English is one sentence, read whole by the Korean model.
    const ranges = (HANGUL.test(line) && sentenceRanges(line, true)) || sentenceRanges(line, false)
    const merged = []
    let pendingStart = null
    for (const [start, end] of ranges) {
        const text = line.slice(start, end)
        if (!text.trim()) {
            continue
        }
        if (!LETTER.test(text)) {
            pendingStart ??= start
            continue
        }
        merged.push([pendingStart ?? start, end])
        pendingStart = null
    }
    if (pendingStart !== null) {
        if (merged.length > 0) {
            merged[merged.length - 1][1] = line.length
        } else {
            merged.push([pendingStart, line.length])
        }
    }
    return merged.map(([start, end]) => line.slice(start, end).trim()).filter(Boolean)
}

// Models cut long input short (the Korean model drops what is past its input limit), so text is
// spoken sentence by sentence, and a very long sentence in chunks of at most 220 characters, cut
// at the last space before that limit (at the limit when there is none). A sentence that took in a
// list number or an emoji is cut the same way. The text is composed (NFC), so a Hangul syllable
// is one character whatever form it was pasted in.
export function splitForSpeech(text) {
    const chunks = []
    const sentences = String(text || '')
        .normalize('NFC')
        .split(/\n+/)
        .flatMap(splitLine)
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
// it quotes English, and is read whole by the Korean voice. A sentence with Latin letters and no
// Hangul is English. Anything else (only Han or kana, only digits, symbols or emoji) is read by
// neither voice: null.
export function detectSpeechLang(text) {
    const composed = String(text || '').normalize('NFC')
    if (HANGUL.test(composed)) {
        return 'ko-KR'
    }
    return LATIN.test(composed) ? 'en-US' : null
}
