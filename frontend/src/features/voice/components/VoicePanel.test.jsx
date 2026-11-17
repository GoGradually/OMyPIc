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
        feedbackEnabled: true,
        aiFeedbackEnabled: true,
        onToggleAiFeedback: vi.fn(),
        autoReplayEnabled: true,
        onToggleAutoReplay: vi.fn(),
        feedbackSummary: 'Claude Opus 5.5',
        ...overrides
    }
}

describe('VoicePanel', () => {
    it('shows the current question and the feedback step when AI feedback is on', () => {
        render(<VoicePanel {...buildProps()} />)

        expect(screen.getByText('Tell me about your hometown.')).toBeTruthy()
        expect(screen.getByText('피드백')).toBeTruthy()
        expect(screen.getByText('Claude Opus 5.5로 첨삭합니다.')).toBeTruthy()
        expect(screen.getByRole('switch', {name: 'AI 피드백'}).getAttribute('aria-checked')).toBe('true')
    })

    it('turns AI feedback off from the switch', () => {
        const onToggleAiFeedback = vi.fn()
        render(<VoicePanel {...buildProps({onToggleAiFeedback})} />)

        fireEvent.click(screen.getByRole('switch', {name: 'AI 피드백'}))

        expect(onToggleAiFeedback).toHaveBeenCalledWith(false)
    })

    it('replaces the feedback step with answer replay when AI feedback is off', () => {
        const onToggleAutoReplay = vi.fn()
        render(<VoicePanel {...buildProps({
            feedbackEnabled: false,
            aiFeedbackEnabled: false,
            onToggleAutoReplay
        })} />)

        expect(screen.queryByText('피드백')).toBeNull()
        expect(screen.getByText('내 응답 다시듣기')).toBeTruthy()
        fireEvent.click(screen.getByRole('checkbox', {name: '답변 후 내 응답 자동 재생'}))
        expect(onToggleAutoReplay).toHaveBeenCalledWith(false)
    })

    it('locks the switch during a session and marks the active step', () => {
        render(<VoicePanel {...buildProps({
            sessionActive: true,
            voiceConnected: true,
            speechState: 'CAPTURING_ANSWER'
        })} />)

        expect(screen.getByRole('switch', {name: 'AI 피드백'}).disabled).toBe(true)
        expect(screen.getByText('답변').closest('li').getAttribute('aria-current')).toBe('step')
        expect(screen.getByText(/답변을 듣고 있습니다/)).toBeTruthy()
    })
})
