// The line the voice panel keeps on screen while an on-device voice the session uses cannot speak
// yet: loading in the background, or failed. Status-bar messages come and go with every sentence,
// so this is shown apart from them, for as long as it is true.

const KOREAN_SIZE = '약 263MB'

function englishNote({status}) {
    if (status === 'failed') {
        return '음성 합성 모델을 불러오지 못해 질문을 화면으로만 보여 드립니다.'
    }
    if (status === 'loading') {
        // Session start waits for the first load; this is a reload in the background.
        return '영어 음성 합성 모델을 다시 불러오고 있습니다. 그동안 질문은 화면으로만 보여 드립니다.'
    }
    return ''
}

function koreanNote({status, progress}) {
    if (status === 'failed') {
        return '한국어 음성 모델을 불러오지 못해 한국어 피드백은 화면으로만 보여 드립니다.'
    }
    if (status === 'loading') {
        const percent = Number.isFinite(progress) ? ` (${Math.floor(progress)}%)` : ''
        return `한국어 음성 모델을 준비하고 있습니다.${percent} 처음 사용할 때는 ${KOREAN_SIZE}를 내려받습니다. 준비될 때까지 한국어 피드백은 소리 없이 화면으로만 보여 드립니다.`
    }
    return ''
}

/**
 * english and korean are {status, progress} of the two on-device voices. localSpeech: the session
 * reads its speech on this device; koreanWanted: it has Korean feedback. Returns '' when there is
 * nothing to say.
 */
export function speechModelNote({english, korean, localSpeech, koreanWanted}) {
    if (!localSpeech) {
        return ''
    }
    return [englishNote(english || {}), koreanWanted ? koreanNote(korean || {}) : '']
        .filter(Boolean)
        .join(' ')
}
