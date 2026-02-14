/** @vitest-environment jsdom */
import React from 'react'
import {afterEach, describe, expect, it, vi} from 'vitest'
import {cleanup, render, screen} from '@testing-library/react'
import {LearningModeOverlay} from './LearningModeOverlay.jsx'

afterEach(() => {
    cleanup()
})

function buildProps(overrides = {}) {
    return {
        mode: 'IMMEDIATE',
        setMode: vi.fn(),
        batchSize: 3,
        setBatchSize: vi.fn(),
        updateMode: vi.fn(),
        tagStats: [],
        selectedGroupTags: [],
        toggleSelectedTag: vi.fn(),
        ...overrides
    }
}

describe('LearningModeOverlay', () => {
    it('renders tag buttons from tag stats', () => {
        render(
            <LearningModeOverlay
                {...buildProps({
                    tagStats: [
                        {tag: 'travel', groupCount: 2, selectable: true},
                        {tag: 'habit', groupCount: 1, selectable: true}
                    ]
                })}
            />
        )

        expect(screen.getByRole('button', {name: 'travel (2)'})).toBeTruthy()
        expect(screen.getByRole('button', {name: 'habit (1)'})).toBeTruthy()
    })

    it('disables non-selectable tags', () => {
        render(
            <LearningModeOverlay
                {...buildProps({
                    tagStats: [
                        {tag: 'travel', groupCount: 2, selectable: true},
                        {tag: 'legacy', groupCount: 0, selectable: false}
                    ]
                })}
            />
        )

        const enabled = screen.getByRole('button', {name: 'travel (2)'})
        const disabled = screen.getByRole('button', {name: 'legacy (0)'})
        expect(enabled.disabled).toBe(false)
        expect(disabled.disabled).toBe(true)
    })

    it('does not render question manager controls', () => {
        render(<LearningModeOverlay {...buildProps()} />)

        expect(screen.queryByText('새 질문 그룹 이름')).toBeNull()
        expect(screen.queryByRole('button', {name: '다음 질문'})).toBeNull()
    })
})
