import {describe, expect, it} from 'vitest'
import {detectSpeechLang, splitForSpeech} from './speechText.js'

describe('speech text', () => {
    it('splits text into sentences and breaks very long ones', () => {
        expect(splitForSpeech('Tell me about your home. What does it look like?\nThank you!'))
            .toEqual(['Tell me about your home.', 'What does it look like?', 'Thank you!'])
        const long = Array.from({length: 80}, () => 'word').join(' ')
        expect(splitForSpeech(long).every((chunk) => chunk.length <= 220)).toBe(true)
        expect(splitForSpeech('   ')).toEqual([])
    })

    it('splits a 500-character Korean sentence into chunks of at most 220', () => {
        const sentence = `${Array.from({length: 100}, () => '좋은 답변').join(' ')}입니다.`
        expect(sentence.length).toBeGreaterThanOrEqual(500)

        const chunks = splitForSpeech(sentence)

        expect(chunks.length).toBeGreaterThanOrEqual(3)
        expect(chunks.every((chunk) => chunk.length > 0 && chunk.length <= 220)).toBe(true)
        expect(chunks.join(' ')).toBe(sentence)
    })

    it('detects the language of a sentence', () => {
        expect(detectSpeechLang('Tell me about your home.')).toBe('en-US')
        expect(detectSpeechLang('과거 시제를 써야 합니다.')).toBe('ko-KR')
    })

    it('detects Korean when an English quote is embedded', () => {
        expect(detectSpeechLang('"I goed" 대신 "I went"라고 해야 합니다.')).toBe('ko-KR')
    })
})
