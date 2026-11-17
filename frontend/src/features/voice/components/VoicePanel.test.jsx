/** @vitest-environment jsdom */
import React from 'react'
import {afterEach, describe, expect, it, vi} from 'vitest'
import {cleanup, fireEvent, render, screen} from '@testing-library/react'
import {VoicePanel} from './VoicePanel.jsx'

afterEach(() => {
    cleanup()
})

function buildProps(overrides = {}) {
    return {
        voiceConnected: false,
        sessionActive: false,
        speechState: 'IDLE',
        partialTranscript: '',
        startSession: vi.fn(),
        stopSession: vi.fn(),
        audioConnectionReady: true,
        audioConnectionLabel: '마이크 연결됨',
        audioPermissionLabel: '허용됨',
        audioQuickHint: '입력 1개, 출력 1개',
        audioPermission: 'granted',
        audioDeviceStatus: {lastCheckedAt: ''},
        handleAudioQuickAction: vi.fn(),
        replayButtonLabel: '내 응답 재생',
        replayButtonDisabled: true,
        replayButtonDisabledReason: '다시 들을 수 있는 응답이 없습니다.',
        handleReplayAction: vi.fn(),
        currentQuestionLabel: 'Tell me about your hometown.',
        hasQuestion: true,
        stageAiEnabled: true,
        aiEnabled: true,
        onToggleAi: vi.fn(),
        autoReplayEnabled: true,
        onToggleAutoReplay: vi.fn(),
        feedbackSummary: 'Claude Opus 5.5',
        ...overrides
    }
}

describe('VoicePanel', () => {
    it('shows the current question and the feedback step when AI is on', () => {
        render(<VoicePanel {...buildProps()} />)

        expect(screen.getByText('Tell me about your hometown.')).toBeTruthy()
        expect(screen.getByText('피드백')).toBeTruthy()
        expect(screen.getByText('질문을 읽어 주고, 답변을 Claude Opus 5.5로 첨삭합니다.')).toBeTruthy()
        expect(screen.getByRole('switch', {name: 'AI 사용'}).getAttribute('aria-checked')).toBe('true')
    })

    it('turns AI off from the switch', () => {
        const onToggleAi = vi.fn()
        render(<VoicePanel {...buildProps({onToggleAi})} />)

        fireEvent.click(screen.getByRole('switch', {name: 'AI 사용'}))

        expect(onToggleAi).toHaveBeenCalledWith(false)
    })

    it('shows reading and replay steps and needs no API key when AI is off', () => {
        const onToggleAutoReplay = vi.fn()
        render(<VoicePanel {...buildProps({
            stageAiEnabled: false,
            aiEnabled: false,
            onToggleAutoReplay
        })} />)

        expect(screen.queryByText('피드백')).toBeNull()
        expect(screen.getByText('질문 읽기')).toBeTruthy()
        expect(screen.getByText('내 응답 다시듣기')).toBeTruthy()
        expect(screen.getByText(/API Key 없이 진행합니다/)).toBeTruthy()
        fireEvent.click(screen.getByRole('checkbox', {name: '답변 후 내 응답 자동 재생'}))
        expect(onToggleAutoReplay).toHaveBeenCalledWith(false)
    })

    it('locks the switch during a session and marks the active step', () => {
        render(<VoicePanel {...buildProps({
            sessionActive: true,
            voiceConnected: true,
            speechState: 'CAPTURING_ANSWER'
        })} />)

        expect(screen.getByRole('switch', {name: 'AI 사용'}).disabled).toBe(true)
        expect(screen.getByText('답변').closest('li').getAttribute('aria-current')).toBe('step')
        expect(screen.getByText(/답변을 듣고 있습니다/)).toBeTruthy()
    })
})
