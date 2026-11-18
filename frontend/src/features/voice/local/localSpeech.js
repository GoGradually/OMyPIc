// Reads text aloud without calling any API.
//
// Each sentence is spoken in its own language by one of two free engines:
//   - the device voice (instant, but only for languages the OS has a voice for)
//   - the on-device Kokoro model (native English on any machine; slower, because it synthesizes
//     on the CPU, and it needs a one-time model download)
//
// engine 'device': device voice; English falls back to Kokoro when the OS has no English voice.
// engine 'kokoro': English always uses Kokoro. Korean always uses the device voice.

import {pcm16ToWav} from '../../../shared/utils/audioCodec.js'
import {detectSpeechLang, loadDeviceVoices, pickVoice, speakSentence, splitForSpeech} from './deviceVoice.js'
import {isLocalTtsSupported, loadLocalTts, synthesizeEnglish} from './localTts.js'

export const SPEECH_ENGINE = {
    DEVICE: 'device',
    KOKORO: 'kokoro'
}

const CACHE_LIMIT = 60
// sentence -> Promise<{samples, sampleRate} | null>. Questions are read twice, so the second
// pass and any sentence prepared ahead of time play without waiting.
const synthesisCache = new Map()

function synthesizeCached(sentence) {
    if (synthesisCache.has(sentence)) {
        return synthesisCache.get(sentence)
    }
    const pending = synthesizeEnglish(sentence).catch(() => {
        synthesisCache.delete(sentence)
        return null
    })
    synthesisCache.set(sentence, pending)
    if (synthesisCache.size > CACHE_LIMIT) {
        synthesisCache.delete(synthesisCache.keys().next().value)
    }
    return pending
}

async function buildPlan(text, engine) {
    const voices = await loadDeviceVoices()
    return splitForSpeech(text).map((sentence) => {
        const lang = detectSpeechLang(sentence)
        const voice = pickVoice(voices, lang)
        const english = lang.startsWith('en')
        const useModel = english && isLocalTtsSupported() && (engine === SPEECH_ENGINE.KOKORO || !voice)
        return {sentence, voice, useModel, fallback: useModel && engine !== SPEECH_ENGINE.KOKORO}
    })
}

/** Whether this device needs the Kokoro model to read English with the given engine. */
export async function needsLocalTtsModel(engine) {
    if (!isLocalTtsSupported()) {
        return false
    }
    if (engine === SPEECH_ENGINE.KOKORO) {
        return true
    }
    return !pickVoice(await loadDeviceVoices(), 'en-US')
}

/** Starts synthesizing ahead of time so the speech is ready when its turn comes. */
export async function prepareLocalSpeech(text, engine) {
    const plan = await buildPlan(text, engine)
    plan.filter((step) => step.useModel).forEach((step) => synthesizeCached(step.sentence))
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
 * Speaks the text and calls onEnd when at least part of it was heard, or onUnavailable when
 * nothing could be spoken on this device. onNotice reports 'no-english-voice' (falling back to
 * the model), 'preparing' (waiting for synthesis) and 'ready' (synthesis done, playback starts).
 * Returns a function that cancels silently.
 */
export function speakLocally(text, {engine = SPEECH_ENGINE.DEVICE, onEnd, onUnavailable, onNotice} = {}) {
    let cancelled = false
    let active = null

    const run = async () => {
        const plan = await buildPlan(text, engine)
        if (cancelled) {
            return
        }
        if (plan.some((step) => step.fallback)) {
            onNotice?.('no-english-voice')
        }
        // The whole text is synthesized before it starts, so a question plays without pauses
        // between its sentences.
        const modelSteps = plan.filter((step) => step.useModel)
        if (modelSteps.length > 0) {
            onNotice?.('preparing')
            for (const step of modelSteps) {
                step.audio = await synthesizeCached(step.sentence)
                if (cancelled) {
                    return
                }
            }
            onNotice?.('ready')
        }

        let spoken = 0
        for (const step of plan) {
            if (step.useModel) {
                active = step.audio ? playSamples(step.audio) : null
            } else {
                active = step.voice ? speakSentence(step.sentence, step.voice) : null
            }
            if (active && await active.done) {
                spoken += 1
            }
            if (cancelled) {
                return
            }
        }
        active = null
        if (plan.length > 0 && spoken === 0) {
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

export {loadLocalTts}
