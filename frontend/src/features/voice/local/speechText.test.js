import {describe, expect, it} from 'vitest'
import {detectSpeechLang, splitForSpeech} from './speechText.js'

// Each chunk with the language that reads it (null: nobody).
function chunksOf(text) {
    return splitForSpeech(text).map((chunk) => [chunk, detectSpeechLang(chunk)])
}

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

describe('sentence boundaries', () => {
    it('keeps a Korean sentence that quotes English sentences whole', () => {
        expect(chunksOf('"I goed to the park." 대신 "I went to the park."라고 해야 합니다.')).toEqual([
            ['"I goed to the park." 대신 "I went to the park."라고 해야 합니다.', 'ko-KR']
        ])
        expect(chunksOf('Grammar: "I goed to the cafe yesterday."는 "I went to the cafe yesterday."로 고쳐야 합니다. 불규칙 동사를 확인하세요.')).toEqual([
            ['Grammar: "I goed to the cafe yesterday."는 "I went to the cafe yesterday."로 고쳐야 합니다.', 'ko-KR'],
            ['불규칙 동사를 확인하세요.', 'ko-KR']
        ])
        expect(chunksOf('"Do you like it?"라고 물어보는 것이 자연스럽습니다.')).toEqual([
            ['"Do you like it?"라고 물어보는 것이 자연스럽습니다.', 'ko-KR']
        ])
    })

    it('keeps curly and single-quoted spans whole on a Korean line, apostrophes included', () => {
        expect(chunksOf('“I don’t know.”라고 말해도 됩니다. 괜찮습니다.')).toEqual([
            ['“I don’t know.”라고 말해도 됩니다.', 'ko-KR'],
            ['괜찮습니다.', 'ko-KR']
        ])
        expect(chunksOf("'I goed home.' 대신 'I went home.'이라고 하세요. 'Don't worry.'도 좋습니다.")).toEqual([
            ["'I goed home.' 대신 'I went home.'이라고 하세요.", 'ko-KR'],
            ["'Don't worry.'도 좋습니다.", 'ko-KR']
        ])
    })

    it('splits normally on a Korean line whose quote is never closed', () => {
        expect(splitForSpeech('그는 "좋아요. 라고 했습니다. 다음 문장입니다.'))
            .toEqual(['그는 "좋아요.', '라고 했습니다.', '다음 문장입니다.'])
    })

    it('ends an English sentence after a closing quote or bracket', () => {
        expect(splitForSpeech('He said "Stop." Then he left. (It was late.) We went home.'))
            .toEqual(['He said "Stop."', 'Then he left.', '(It was late.)', 'We went home.'])
    })

    it('keeps a quoted span whole on a line without Hangul, and ends the sentence after it when a new one starts', () => {
        expect(splitForSpeech('He said "Stop. Now." and left. Then he came back.'))
            .toEqual(['He said "Stop. Now." and left.', 'Then he came back.'])
        expect(splitForSpeech('She asked, “Are you sure? Really?” I nodded.'))
            .toEqual(['She asked, “Are you sure? Really?”', 'I nodded.'])
        expect(splitForSpeech("I don't know. She said 'Wait. Please.' and smiled."))
            .toEqual(["I don't know.", "She said 'Wait. Please.' and smiled."])
    })

    it('keeps a long quoted span within the 220-character limit', () => {
        const quote = Array.from({length: 30}, (_, index) => `Sentence ${index} is here.`).join(' ')
        const chunks = splitForSpeech(`He read "${quote}" aloud.`)

        expect(chunks.every((chunk) => chunk.length <= 220)).toBe(true)
        expect(chunks.join(' ')).toBe(`He read "${quote}" aloud.`)
    })

    it('splits normally on a line without Hangul whose quote is never closed', () => {
        expect(splitForSpeech('He said "Stop. Then he left.')).toEqual(['He said "Stop.', 'Then he left.'])
    })

    it('reads the quoted English start of a Korean sentence with it when the quote is never closed', () => {
        expect(chunksOf('"I goed to the park. 대신 I went라고 해야 합니다. 다음 문장입니다.')).toEqual([
            ['"I goed to the park. 대신 I went라고 해야 합니다.', 'ko-KR'],
            ['다음 문장입니다.', 'ko-KR']
        ])
        expect(chunksOf('잘했어요. "Thank you.')).toEqual([['잘했어요. "Thank you.', 'ko-KR']])
        // An English sentence of its own on a Korean line keeps the English voice.
        expect(chunksOf('그는 "좋아요. I like it. 라고 했습니다.')).toEqual([
            ['그는 "좋아요.', 'ko-KR'],
            ['I like it.', 'en-US'],
            ['라고 했습니다.', 'ko-KR']
        ])
    })

    it('never splits between two digits', () => {
        expect(chunksOf('전체 점수는 3.5점 정도이고 IH 수준입니다.')).toEqual([
            ['전체 점수는 3.5점 정도이고 IH 수준입니다.', 'ko-KR']
        ])
        expect(splitForSpeech('I order an iced americano, which costs about $4.50. The owner, Mr. Lee, is very friendly.'))
            .toEqual(['I order an iced americano, which costs about $4.50.', 'The owner, Mr. Lee, is very friendly.'])
    })

    it('does not split after common abbreviations', () => {
        expect(splitForSpeech('Use linking words, e.g. however, i.e. a contrast. Ask Dr. Kim or Mrs. Park vs. Ms. Choi.'))
            .toEqual(['Use linking words, e.g. however, i.e. a contrast.', 'Ask Dr. Kim or Mrs. Park vs. Ms. Choi.'])
        expect(splitForSpeech('I like tea, coffee, etc. and juice. I live in the U.S. now.'))
            .toEqual(['I like tea, coffee, etc. and juice.', 'I live in the U.S. now.'])
        expect(splitForSpeech('I bought apples, pears, etc. Then I went home.'))
            .toEqual(['I bought apples, pears, etc.', 'Then I went home.'])
    })

    it('does not split after No. before a number', () => {
        expect(splitForSpeech('It is No. 3 on the list. I said no. Then I left.'))
            .toEqual(['It is No. 3 on the list.', 'I said no.', 'Then I left.'])
    })

    it('does not split inside a number, an address or a time', () => {
        expect(splitForSpeech('See https://example.com/a.b?c=d for more. 오후 3:30에 만나요.'))
            .toEqual(['See https://example.com/a.b?c=d for more.', '오후 3:30에 만나요.'])
    })

    it('keeps a run of terminators with its sentence', () => {
        expect(splitForSpeech('음... 좋았어요!! 정말요?')).toEqual(['음...', '좋았어요!!', '정말요?'])
    })
})

describe('chunks without letters', () => {
    it('merges a list number into the sentence after it', () => {
        expect(chunksOf('1. 문법: 과거 시제를 쓰세요.\n2. Tell me about your home.')).toEqual([
            ['1. 문법: 과거 시제를 쓰세요.', 'ko-KR'],
            ['2. Tell me about your home.', 'en-US']
        ])
    })

    it('merges a trailing number or symbol into the sentence before it on the same line', () => {
        expect(chunksOf('잘했어요. 👍')).toEqual([['잘했어요. 👍', 'ko-KR']])
        expect(chunksOf('Thank you. 2. 3.')).toEqual([['Thank you. 2. 3.', 'en-US']])
    })

    it('leaves a line without letters to nobody', () => {
        expect(chunksOf('좋았습니다.\n👍\n3.')).toEqual([
            ['좋았습니다.', 'ko-KR'],
            ['👍', null],
            ['3.', null]
        ])
    })

    it('cuts a merged chunk longer than 220 characters like any long sentence', () => {
        const sentence = `1. ${Array.from({length: 60}, () => '좋은 답변').join(' ')}입니다.`
        const chunks = splitForSpeech(sentence)

        expect(chunks[0].startsWith('1. 좋은 답변')).toBe(true)
        expect(chunks.every((chunk) => chunk.length <= 220)).toBe(true)
        expect(chunks.join(' ')).toBe(sentence)
        expect(chunks.every((chunk) => detectSpeechLang(chunk) === 'ko-KR')).toBe(true)
    })
})

describe('language detection', () => {
    it('reads decomposed Hangul as Korean and gives the models composed text', () => {
        const decomposed = '한국어 문장입니다.'.normalize('NFD')
        expect(decomposed).not.toBe('한국어 문장입니다.')

        expect(detectSpeechLang(decomposed)).toBe('ko-KR')
        expect(splitForSpeech(decomposed)).toEqual(['한국어 문장입니다.'])
    })

    it('leaves Han-only, kana-only and emoji-only chunks to nobody', () => {
        expect(detectSpeechLang('文法注意。')).toBeNull()
        expect(detectSpeechLang('ありがとう。')).toBeNull()
        expect(detectSpeechLang('👍')).toBeNull()
        expect(detectSpeechLang('1.')).toBeNull()
        expect(detectSpeechLang('文法에 注意하세요.')).toBe('ko-KR')
        expect(detectSpeechLang('Café au lait.')).toBe('en-US')
    })
})

describe('feedback text', () => {
    // The shape the server sends: summary, correction lines, recommendation lines, example answer.
    const FEEDBACK = [
        '전반적으로 질문에 맞게 답했고 내용 전달도 좋았습니다. 다만 과거 시제에서 실수가 반복되었습니다.',
        'Grammar: "I goed to the cafe yesterday."는 "I went to the cafe yesterday."로 고쳐야 합니다.',
        'Expression: "very good" 대신 "excellent"처럼 더 구체적인 형용사를 써 보세요.',
        'Logic: 이유를 말한 뒤에 예시를 1가지 이상 덧붙이면 더 설득력 있게 들립니다.',
        '1. Filler: well - 생각할 시간이 필요할 때 문장 앞에 넣어 보세요.',
        '2. Adjective: cozy - 카페 분위기를 묘사할 때 씁니다.',
        '3. Adverb: usually - 습관을 말할 때 "I usually order tea."처럼 씁니다.',
        'I usually go to a small cafe near my house. It has a cozy atmosphere. I went there yesterday and ordered an iced americano for $4.50. Well, the coffee was excellent. I often read a book there. I really recommend this place.'
    ].join('\n')

    it('is read in on-screen order, Korean lines whole by the Korean model and the example answer by sentence in English', () => {
        expect(chunksOf(FEEDBACK)).toEqual([
            ['전반적으로 질문에 맞게 답했고 내용 전달도 좋았습니다.', 'ko-KR'],
            ['다만 과거 시제에서 실수가 반복되었습니다.', 'ko-KR'],
            ['Grammar: "I goed to the cafe yesterday."는 "I went to the cafe yesterday."로 고쳐야 합니다.', 'ko-KR'],
            ['Expression: "very good" 대신 "excellent"처럼 더 구체적인 형용사를 써 보세요.', 'ko-KR'],
            ['Logic: 이유를 말한 뒤에 예시를 1가지 이상 덧붙이면 더 설득력 있게 들립니다.', 'ko-KR'],
            ['1. Filler: well - 생각할 시간이 필요할 때 문장 앞에 넣어 보세요.', 'ko-KR'],
            ['2. Adjective: cozy - 카페 분위기를 묘사할 때 씁니다.', 'ko-KR'],
            ['3. Adverb: usually - 습관을 말할 때 "I usually order tea."처럼 씁니다.', 'ko-KR'],
            ['I usually go to a small cafe near my house.', 'en-US'],
            ['It has a cozy atmosphere.', 'en-US'],
            ['I went there yesterday and ordered an iced americano for $4.50.', 'en-US'],
            ['Well, the coffee was excellent.', 'en-US'],
            ['I often read a book there.', 'en-US'],
            ['I really recommend this place.', 'en-US']
        ])
    })
})
