import {useCallback, useEffect, useState} from 'react'
import {getApiKey, setApiKey, verifyApiKey} from '../../shared/api/http.js'
import {FEEDBACK_MODELS, getModeSummary, PANEL_TITLES, VOICE_STT_MODELS, VOICES} from '../../shared/constants/models.js'
import {copyText} from '../../shared/utils/clipboard.js'
import {getCurrentQuestionLabel} from '../../shared/utils/mode.js'
import {useSessionId} from '../providers/session.js'
import {useVoiceSession} from '../../features/voice/hooks/useVoiceSession.js'
import {useRulebooks} from '../../features/rulebooks/hooks/useRulebooks.js'
import {useWrongNotes} from '../../features/wrongnotes/hooks/useWrongNotes.js'
import {useQuestionGroups} from '../../features/questions/hooks/useQuestionGroups.js'
import {getAudioUiState} from '../../features/voice/utils/audioStatus.js'

export function useAppViewModel() {
    const sessionId = useSessionId()

    const [feedbackModel, setFeedbackModel] = useState(FEEDBACK_MODELS[0])
    const [voiceSttModel, setVoiceSttModel] = useState(VOICE_STT_MODELS[0])

    const [apiKeyInput, setApiKeyInput] = useState('')
    const [statusMessage, setStatusMessage] = useState('')
    const [feedbackLang, setFeedbackLang] = useState('ko')
    const [voice, setVoice] = useState(VOICES[0])

    const [feedback, setFeedback] = useState(null)
    const [activePanel, setActivePanel] = useState('')
    const [showStatusDetails, setShowStatusDetails] = useState(false)

    const {
        rulebooks,
        enabledRulebookCount,
        refreshRulebooks,
        uploadRulebook,
        toggleRulebook,
        deleteRulebook
    } = useRulebooks()

    const {
        wrongNotes,
        refreshWrongNotes
    } = useWrongNotes()

    const {
        questionGroups,
        tagStats,
        selectedGroupTags,
        toggleSelectedTag,
        activeGroupId,
        setActiveGroupId,
        activeQuestionGroup,
        newGroupName,
        setNewGroupName,
        newGroupTagsInput,
        setNewGroupTagsInput,
        newQuestion,
        setNewQuestion,
        newQuestionType,
        setNewQuestionType,
        editingQuestionId,
        editingQuestionText,
        setEditingQuestionText,
        editingQuestionType,
        setEditingQuestionType,
        mode,
        setMode,
        batchSize,
        setBatchSize,
        currentQuestion,
        refreshQuestionGroups,
        createGroup,
        deleteGroup,
        addQuestion,
        startEditQuestion,
        cancelEditQuestion,
        saveEditedQuestion,
        removeQuestion,
        updateMode,
        setCurrentQuestion
    } = useQuestionGroups({
        sessionId,
        onStatus: setStatusMessage
    })

    const {
        sessionActive,
        voiceConnected,
        speechState,
        partialTranscript,
        transcript,
        userText,
        audioPermission,
        audioDeviceStatus,
        startSession,
        stopSession,
        handleAudioQuickAction
    } = useVoiceSession({
        sessionId,
        feedbackModel,
        voiceSttModel,
        feedbackLang,
        voice,
        onStatus: setStatusMessage,
        onFeedback: setFeedback,
        refreshWrongNotes,
        onQuestionPrompt: setCurrentQuestion
    })

    useEffect(() => {
        refreshRulebooks().catch(() => {
        })
        refreshQuestionGroups().catch(() => {
        })
        refreshWrongNotes().catch(() => {
        })
    }, [refreshRulebooks, refreshQuestionGroups, refreshWrongNotes])

    useEffect(() => {
        getApiKey().then((key) => setApiKeyInput(key || ''))
        setStatusMessage('')
    }, [])

    const handleSaveApiKey = useCallback(async () => {
        await setApiKey(apiKeyInput)
        try {
            const result = await verifyApiKey(apiKeyInput, feedbackModel)
            if (result.valid) {
                setStatusMessage('API Key 검증이 완료되었습니다.')
            } else {
                setStatusMessage(`API Key 검증 실패: ${result.message}`)
            }
        } catch (error) {
            setStatusMessage(`API Key 검증 실패: ${error.message}`)
        }
    }, [apiKeyInput, feedbackModel])

    const copyUserText = useCallback(async () => {
        if (!userText.trim()) {
            return
        }
        try {
            await copyText(userText)
            setStatusMessage('인식 텍스트를 복사했습니다.')
        } catch (_error) {
            setStatusMessage('텍스트 복사에 실패했습니다.')
        }
    }, [userText])

    const togglePanel = useCallback((panelName) => {
        setActivePanel((prev) => (prev === panelName ? '' : panelName))
    }, [])

    const handleStartSession = useCallback(async () => {
        try {
            await updateMode()
            await startSession()
        } catch (error) {
            setStatusMessage(error?.message || '세션 시작에 실패했습니다.')
        }
    }, [updateMode, startSession])

    const handleStopSession = useCallback(async () => {
        await stopSession({forced: true, reason: 'user_stop', statusMessage: '세션을 종료했습니다.'})
    }, [stopSession])

    const currentQuestionLabel = getCurrentQuestionLabel(currentQuestion)
    const modeSummary = getModeSummary(mode, batchSize)
    const {
        audioConnectionLabel,
        audioConnectionReady,
        audioPermissionLabel,
        audioQuickHint
    } = getAudioUiState({
        audioPermission,
        audioDeviceStatus,
        recording: sessionActive
    })

    const statusDetails = {
        sessionIdPrefix: sessionId.slice(0, 8),
        audioPermissionLabel,
        audioInputCount: audioDeviceStatus.inputCount,
        voiceSttModel,
        feedbackModel
    }

    const rulebookProps = {
        rulebooks,
        uploadRulebook,
        toggleRulebook,
        deleteRulebook
    }

    const questionGroupManagerProps = {
        activeGroupId,
        newGroupName,
        setNewGroupName,
        newGroupTagsInput,
        setNewGroupTagsInput,
        questionGroups,
        setActiveGroupId,
        createGroup,
        deleteGroup
    }

    const groupQuestionManagerProps = {
        activeGroupId,
        questionGroups,
        setActiveGroupId,
        newQuestion,
        setNewQuestion,
        newQuestionType,
        setNewQuestionType,
        addQuestion,
        activeQuestionGroup,
        editingQuestionId,
        editingQuestionText,
        setEditingQuestionText,
        editingQuestionType,
        setEditingQuestionType,
        startEditQuestion,
        saveEditedQuestion,
        cancelEditQuestion,
        removeQuestion
    }

    const learningModeProps = {
        mode,
        setMode,
        batchSize,
        setBatchSize,
        updateMode,
        tagStats,
        selectedGroupTags,
        toggleSelectedTag
    }

    const wrongNotesProps = {
        wrongNotes,
        feedback
    }

    const modelSettingsProps = {
        voiceSttModel,
        setVoiceSttModel,
        feedbackModel,
        setFeedbackModel,
        apiKeyInput,
        setApiKeyInput,
        voice,
        setVoice,
        feedbackLang,
        setFeedbackLang,
        onSaveApiKey: handleSaveApiKey
    }

    return {
        activePanel,
        setActivePanel,
        togglePanel,
        showStatusDetails,
        setShowStatusDetails,
        overlayTitle: PANEL_TITLES[activePanel],
        overlayContentProps: {
            activePanel,
            rulebookProps,
            questionGroupManagerProps,
            groupQuestionManagerProps,
            learningModeProps,
            wrongNotesProps,
            modelSettingsProps
        },
        voicePanelProps: {
            voiceConnected,
            sessionActive,
            speechState,
            partialTranscript,
            startSession: handleStartSession,
            stopSession: handleStopSession,
            audioConnectionReady,
            audioConnectionLabel,
            audioPermissionLabel,
            audioQuickHint,
            audioPermission,
            audioDeviceStatus,
            handleAudioQuickAction
        },
        questionPanelProps: {
            currentQuestionLabel,
            modeSummary,
            onOpenQuestionGroupManager: () => setActivePanel('question-group-manager'),
            onOpenGroupQuestionManager: () => setActivePanel('group-question-manager'),
            onOpenLearningMode: () => setActivePanel('learning-mode')
        },
        transcriptPanelProps: {
            userText,
            transcript,
            onCopyUserText: copyUserText
        },
        summaryPanelProps: {
            feedbackLang,
            voice,
            voiceSttModel,
            feedbackModel,
            enabledRulebookCount,
            questionGroupCount: questionGroups.length,
            onOpenSettings: () => setActivePanel('model')
        },
        recentFeedbackPanelProps: {
            feedback,
            onOpenWrongnotes: () => setActivePanel('wrongnotes')
        },
        statusBarProps: {
            voiceConnected,
            audioConnectionLabel,
            statusMessage,
            showStatusDetails,
            onToggleStatusDetails: () => setShowStatusDetails((prev) => !prev),
            statusDetails
        }
    }
}
