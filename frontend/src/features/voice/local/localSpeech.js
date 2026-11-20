// Reads text aloud without calling any API.
//
// Each sentence is read in its own language by an on-device model: English by Kokoro, Korean
// (any sentence with Hangul, quoted English included) by the Korean model. A sentence whose model
// cannot be used is shown on screen without sound, and the rest still plays.

import {pcm16ToWav} from '../../../shared/utils/audioCodec.js'
import {detectSpeechLang, splitForSpeech} from './speechText.js'
import {isLocalTtsSupported, loadLocalTts, synthesizeEnglish} from './localTts.js'
import {getLocalKoTtsStatus, isLocalKoTtsSupported, loadLocalKoTts, synthesizeKorean} from './localKoTts.js'

const KOREAN = 'ko-KR'
const ENGLISH = 'en-US'
const CACHE_LIMIT = 60
// sentence -> Promise<{samples, sampleRate} | null>. Questions are read twice, so the second
// pass and any sentence prepared ahead of time play without waiting.
const synthesisCache = new Map()

function synthesizeCached(sentence, synthesize) {
    if (synthesisCache.has(sentence)) {
        return synthesisCache.get(sentence)
    }
    const pending = synthesize(sentence).catch(() => {
        synthesisCache.delete(sentence)
        return null
    })
    synthesisCache.set(sentence, pending)
    if (synthesisCache.size > CACHE_LIMIT) {
        synthesisCache.delete(synthesisCache.keys().next().value)
    }
    return pending
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

/** One step per sentence: {sentence, lang, synthesize}; synthesize is null when no model can read it. */
function buildPlan(text, allowKoreanModel) {
    let korean
    return splitForSpeech(text).map((sentence) => {
        const lang = detectSpeechLang(sentence)
        if (lang === KOREAN) {
            if (korean === undefined) {
                korean = koreanSynthesizer(allowKoreanModel)
            }
            return {sentence, lang, synthesize: korean}
        }
        if (lang === ENGLISH) {
            return {sentence, lang, synthesize: isLocalTtsSupported() ? synthesizeEnglish : null}
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
 * been heard could be played. onNotice reports 'preparing' (waiting for synthesis), 'ready'
 * (synthesis done, playback starts) and 'korean-unavailable' (Korean sentences shown without
 * sound because the Korean model is not ready). allowKoreanModel lets a Korean sentence start the
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
        // The whole text is synthesized before it starts, so it plays without pauses between its
        // sentences and always in sentence order, whichever model finishes first.
        const modelSteps = plan.filter((step) => step.synthesize)
        if (modelSteps.length > 0) {
            onNotice?.('preparing')
            const audios = await Promise.all(modelSteps.map((step) => synthesizeCached(step.sentence, step.synthesize)))
            if (cancelled) {
                return
            }
            modelSteps.forEach((step, index) => {
                step.audio = audios[index]
            })
            onNotice?.('ready')
        }

        let spoken = 0
        for (const step of plan) {
            active = step.audio ? playSamples(step.audio) : null
            if (active && await active.done) {
                spoken += 1
            }
            if (cancelled) {
                return
            }
        }
        active = null
        // A Korean sentence waiting for its model, or a chunk no voice reads, is expected to be
        // silent; an English sentence that stayed silent means this device could not play the speech.
        const expected = plan.filter((step) => step.synthesize || step.lang === ENGLISH)
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

export {loadLocalKoTts, loadLocalTts}
