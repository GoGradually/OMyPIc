import {describe, expect, it} from 'vitest'
import {getModeSummary} from './models.js'

describe('models constants', () => {
    it('uses question-group wording for continuous mode summary', () => {
        expect(getModeSummary('CONTINUOUS', 3)).toContain('질문 그룹 단위')
    })
})
