// Reads text aloud without calling any API.
//
// Each sentence is read in its own language by an on-device model: English by Kokoro, Korean
// (any sentence with Hangul, quoted English included) by the Korean model. A sentence whose model
// cannot be used is shown on screen without sound, and the rest still plays.

import {pcm16ToWav} from '../../../shared/utils/audioCodec.js'
import {detectSpeechLang, splitForSpeech} from './speechText.js'
import {
    cancelQueuedEnglishSynthesis,
    getLocalTtsStatus,
    isLocalTtsSupported,
    loadLocalTts,
    synthesizeEnglish,
    watchLocalTtsStatus
} from './localTts.js'
import {
    cancelQueuedKoreanSynthesis,
    getLocalKoTtsStatus,
    isLocalKoTtsSupported,
    loadLocalKoTts,
    synthesizeKorean,
    watchLocalKoTtsStatus
} from './localKoTts.js'

const KOREAN = 'ko-KR'
const ENGLISH = 'en-US'
// Synthesized audio kept for reuse, in bytes of samples (about 3 minutes of Korean, 44.1 kHz float).
const CACHE_BYTES = 32 * 1024 * 1024
// sentence -> {promise: Promise<{samples, sampleRate} | null>, settled, bytes}, least recently used
// first. Questions are read twice, so the second pass and any sentence prepared ahead of time play
// without waiting. The cache is bounded by the size of its audio, not by a count: a long Korean
// sentence is several MB.
const synthesisCache = new Map()
let cachedBytes = 0

function dropFromCache(sentence) {
    const entry = synthesisCache.get(sentence)
    if (entry) {
        cachedBytes -= entry.bytes
        synthesisCache.delete(sentence)
    }
}

function trimCache(keep) {
    for (const [sentence, entry] of synthesisCache) {
        if (cachedBytes <= CACHE_BYTES) {
            return
        }
        // An entry still being synthesized holds no audio yet: dropping it frees nothing and
        // would make its sentence be synthesized twice.
        if (entry !== keep && entry.settled) {
            dropFromCache(sentence)
        }
    }
}

function synthesizeCached(sentence, synthesize) {
    const cached = synthesisCache.get(sentence)
    if (cached) {
        synthesisCache.delete(sentence)
        synthesisCache.set(sentence, cached)
        return cached
    }
    const entry = {settled: false, bytes: 0, promise: null}
    entry.promise = synthesize(sentence).then((audio) => {
        if (synthesisCache.get(sentence) === entry) {
            entry.bytes = audio?.samples?.byteLength || 0
            cachedBytes += entry.bytes
            trimCache(entry)
        }
        return audio
    }).catch(() => {
        if (synthesisCache.get(sentence) === entry) {
            dropFromCache(sentence)
        }
        return null
    }).finally(() => {
        entry.settled = true
    })
    synthesisCache.set(sentence, entry)
    return entry
}

// The Korean model is used only once it is ready. A Korean sentence that comes before that (the
// session did not prepare the model, or it is still downloading) is shown without sound rather
// than holding up the speech queue. Only a session with Korean feedback (allowKoreanModel) then
// starts the load in the background; any other session never downloads the Korean model.
function koreanSynthesizer(allowKoreanModel) {
    if (!isLocalKoTtsSupported()) {
        return null
    }
    const status = getLocalKoTtsStatus()
    if (status === 'ready') {
        return synthesizeKorean
    }
    if (allowKoreanModel && status !== 'loading') {
        loadLocalKoTts().catch(() => {
        })
    }
    return null
}

// Kokoro the same way: used once it is ready. Session start loads it first, so this matters when
// that load failed or the worker crashed: the English sentence is shown without sound, and one
// load is started in the background (not one per sentence, and nothing waits for it).
function englishSynthesizer() {
    if (!isLocalTtsSupported()) {
        return null
    }
    const status = getLocalTtsStatus()
    if (status === 'ready') {
        return synthesizeEnglish
    }
    if (status !== 'loading') {
        loadLocalTts().catch(() => {
        })
    }
    return null
}

/** One step per sentence: {sentence, lang, synthesize}; synthesize is null when no model can read it. */
function buildPlan(text, allowKoreanModel) {
    let korean
    let english
    return splitForSpeech(text).map((sentence) => {
        const lang = detectSpeechLang(sentence)
        if (lang === KOREAN) {
            if (korean === undefined) {
                korean = koreanSynthesizer(allowKoreanModel)
            }
            return {sentence, lang, synthesize: korean}
        }
        if (lang === ENGLISH) {
            if (english === undefined) {
                english = englishSynthesizer()
            }
            return {sentence, lang, synthesize: english}
        }
        // Only digits, symbols or a script neither voice reads: shown on screen only.
        return {sentence, lang, synthesize: null}
    })
}

/**
 * Which on-device models a session needs, given its feedback language (null when there is no
 * feedback). Questions are English; only Korean feedback needs the Korean model.
 */
export function neededLocalTtsModels(feedbackLanguage) {
    return {
        english: isLocalTtsSupported(),
        korean: feedbackLanguage === 'ko' && isLocalKoTtsSupported()
    }
}

/**
 * Starts synthesizing ahead of time so the speech is ready when its turn comes. allowKoreanModel
 * is the same session option as in speakLocally.
 */
export async function prepareLocalSpeech(text, {allowKoreanModel = false} = {}) {
    buildPlan(text, allowKoreanModel)
        .filter((step) => step.synthesize)
        .forEach((step) => synthesizeCached(step.sentence, step.synthesize))
}

/**
 * Stops the synthesis of everything not yet started, for every text spoken or prepared so far
 * (the learner stopped, so nothing queued will be played). Sentences asked for after this call are
 * synthesized afresh, not handed the cancelled requests.
 */
export function cancelLocalSpeech() {
    for (const [sentence, entry] of synthesisCache) {
        if (!entry.settled) {
            dropFromCache(sentence)
        }
    }
    cancelQueuedEnglishSynthesis()
    cancelQueuedKoreanSynthesis()
}

function floatToWavBytes(samples, sampleRate) {
    const pcm = new Int16Array(samples.length)
    for (let index = 0; index < samples.length; index += 1) {
        const value = Math.max(-1, Math.min(1, samples[index]))
        pcm[index] = value < 0 ? value * 32768 : value * 32767
    }
    return pcm16ToWav(new Uint8Array(pcm.buffer), sampleRate, 1)
}

function playSamples(audio) {
    const url = URL.createObjectURL(new Blob([floatToWavBytes(audio.samples, audio.sampleRate)], {type: 'audio/wav'}))
    const element = new Audio(url)
    let finish = () => {
    }
    const done = new Promise((resolve) => {
        let settled = false
        finish = (played) => {
            if (settled) {
                return
            }
            settled = true
            element.onended = null
            element.onerror = null
            URL.revokeObjectURL(url)
            resolve(played)
        }
    })
    element.onended = () => finish(true)
    element.onerror = () => finish(false)
    element.play().catch(() => finish(false))
    return {
        done,
        cancel: () => {
            element.pause()
            finish(false)
        }
    }
}

/**
 * Speaks the text and calls onEnd when it is done, or onUnavailable when nothing that should have
 * been heard could be played. onNotice reports 'preparing' (waiting for the next sentence's
 * synthesis), 'ready' (it plays) and 'korean-unavailable' (Korean sentences shown without sound
 * because the Korean model is not ready); 'preparing' and 'ready' can alternate in a long text. allowKoreanModel lets a Korean sentence start the
 * Korean model download; it is true only in a session with Korean feedback. Returns a function
 * that stops it silently.
 */
export function speakLocally(text, {onEnd, onUnavailable, onNotice, allowKoreanModel = false} = {}) {
    let cancelled = false
    let active = null

    const run = async () => {
        const plan = buildPlan(text, allowKoreanModel)
        if (plan.some((step) => step.lang === KOREAN && !step.synthesize)) {
            onNotice?.('korean-unavailable')
        }
        // Every sentence starts synthesizing at once, and each one plays, in sentence order, as
        // soon as it is ready and the one before it has ended, whichever model finishes first.
        const entries = plan.map((step) => (step.synthesize ? synthesizeCached(step.sentence, step.synthesize) : null))
        let notice = null
        const report = (next) => {
            if (notice !== next) {
                notice = next
                onNotice?.(next)
            }
        }

        let spoken = 0
        for (const entry of entries) {
            if (!entry) {
                continue
            }
            if (!entry.settled) {
                report('preparing')
            }
            const audio = await entry.promise
            if (cancelled) {
                return
            }
            if (!audio) {
                continue
            }
            report('ready')
            active = playSamples(audio)
            if (await active.done) {
                spoken += 1
            }
            if (cancelled) {
                return
            }
        }
        active = null
        // A sentence waiting for its model to load, or a chunk no voice reads, is expected to be
        // silent (the session says so); anything else that stayed silent means this device could
        // not play the speech.
        const englishUnsupported = !isLocalTtsSupported()
        const expected = plan.filter((step) => step.synthesize || (step.lang === ENGLISH && englishUnsupported))
        if (expected.length > 0 && spoken === 0) {
            onUnavailable?.()
        } else {
            onEnd?.()
        }
    }

    run().catch(() => {
        if (!cancelled) {
            onUnavailable?.()
        }
    })

    return () => {
        cancelled = true
        active?.cancel()
    }
}

/**
 * Calls listener({english, korean}) now and whenever one of the two voices changes, each as
 * {status, progress} (status 'idle', 'loading', 'ready' or 'failed'; progress in percent or null).
 * Returns a function that stops it.
 */
export function watchLocalSpeechModels(listener) {
    let models = {
        english: {status: getLocalTtsStatus(), progress: null},
        korean: {status: getLocalKoTtsStatus(), progress: null}
    }
    listener(models)
    const stopEnglish = watchLocalTtsStatus((english) => {
        models = {...models, english}
        listener(models)
    })
    const stopKorean = watchLocalKoTtsStatus((korean) => {
        models = {...models, korean}
        listener(models)
    })
    return () => {
        stopEnglish()
        stopKorean()
    }
}

export {getLocalKoTtsStatus, getLocalTtsStatus, loadLocalKoTts, loadLocalTts}
