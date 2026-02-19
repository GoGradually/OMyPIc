import {useCallback, useEffect, useState} from 'react'
import {getApiKey, getModelMeta, setApiKey, verifyApiKey} from '../../shared/api/http.js'
import {getModeSummary, PANEL_TITLES} from '../../shared/constants/models.js'
import {copyText} from '../../shared/utils/clipboard.js'
import {getCurrentQuestionLabel} from '../../shared/utils/mode.js'
import {useSessionId} from '../providers/session.js'
import {useVoiceSession} from '../../features/voice/hooks/useVoiceSession.js'
import {useRulebooks} from '../../features/rulebooks/hooks/useRulebooks.js'
import {useWrongNotes} from '../../features/wrongnotes/hooks/useWrongNotes.js'
import {useQuestionGroups} from '../../features/questions/hooks/useQuestionGroups.js'
import {getAudioUiState} from '../../features/voice/utils/audioStatus.js'

const EMPTY_MODEL_META = {
    feedbackModels: [],
    defaultFeedbackModel: '',
    voiceSttModels: [],
    defaultVoiceSttModel: '',
    ttsModels: [],
    defaultTtsModel: '',
    voices: [],
    defaultVoice: '',
    feedbackLanguages: ['ko', 'en'],
    defaultFeedbackLanguage: 'ko'
}

function normalizeModelMeta(meta = {}) {
    const feedbackModels = Array.isArray(meta.feedbackModels) ? meta.feedbackModels.filter(Boolean) : []
    const voiceSttModels = Array.isArray(meta.voiceSttModels) ? meta.voiceSttModels.filter(Boolean) : []
    const ttsModels = Array.isArray(meta.ttsModels) ? meta.ttsModels.filter(Boolean) : []
    const voices = Array.isArray(meta.voices) ? meta.voices.filter(Boolean) : []
    const feedbackLanguages = Array.isArray(meta.feedbackLanguages) && meta.feedbackLanguages.length > 0
        ? meta.feedbackLanguages.filter(Boolean)
        : ['ko', 'en']

    return {
        feedbackModels,
        defaultFeedbackModel: typeof meta.defaultFeedbackModel === 'string' ? meta.defaultFeedbackModel : '',
        voiceSttModels,
        defaultVoiceSttModel: typeof meta.defaultVoiceSttModel === 'string' ? meta.defaultVoiceSttModel : '',
        ttsModels,
        defaultTtsModel: typeof meta.defaultTtsModel === 'string' ? meta.defaultTtsModel : '',
        voices,
        defaultVoice: typeof meta.defaultVoice === 'string' ? meta.defaultVoice : '',
        feedbackLanguages,
        defaultFeedbackLanguage: typeof meta.defaultFeedbackLanguage === 'string'
            ? meta.defaultFeedbackLanguage
            : 'ko'
    }
}

function resolveSelectedValue(currentValue, options, preferredDefault) {
    if (currentValue && options.includes(currentValue)) {
        return currentValue
    }
    if (preferredDefault && options.includes(preferredDefault)) {
        return preferredDefault
    }
    return options[0] || currentValue || ''
}

export function useAppViewModel() {
    const sessionId = useSessionId()

    const [feedbackModel, setFeedbackModel] = useState('')
    const [voiceSttModel, setVoiceSttModel] = useState('')
    const [ttsModel, setTtsModel] = useState('')
    const [modelMeta, setModelMeta] = useState(EMPTY_MODEL_META)

    const [apiKeyInput, setApiKeyInput] = useState('')
    const [statusMessage, setStatusMessage] = useState('')
    const [feedbackLang, setFeedbackLang] = useState('ko')
    const [voice, setVoice] = useState('')

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
        isCreateGroupModalOpen,
        newGroupName,
        setNewGroupName,
        newGroupTagsInput,
        setNewGroupTagsInput,
        isEditGroupModalOpen,
        editingGroupName,
        setEditingGroupName,
        editingGroupTagsInput,
        setEditingGroupTagsInput,
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
        openCreateGroupModal,
        closeCreateGroupModal,
        createGroup,
        openEditGroupModal,
        closeEditGroupModal,
        deleteGroup,
        saveEditedGroup,
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
        handleAudioQuickAction,
        replayButtonLabel,
        replayButtonDisabled,
        replayButtonDisabledReason,
        handleReplayAction
    } = useVoiceSession({
        sessionId,
        feedbackModel,
        voiceSttModel,
        ttsModel,
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
        getModelMeta()
            .then((meta) => {
                const normalized = normalizeModelMeta(meta)
                setModelMeta(normalized)
                setFeedbackModel((prev) => resolveSelectedValue(
                    prev,
                    normalized.feedbackModels,
                    normalized.defaultFeedbackModel
                ))
                setVoiceSttModel((prev) => resolveSelectedValue(
                    prev,
                    normalized.voiceSttModels,
                    normalized.defaultVoiceSttModel
                ))
                setTtsModel((prev) => resolveSelectedValue(
                    prev,
                    normalized.ttsModels,
                    normalized.defaultTtsModel
                ))
                setVoice((prev) => resolveSelectedValue(
                    prev,
                    normalized.voices,
                    normalized.defaultVoice
                ))
                setFeedbackLang((prev) => resolveSelectedValue(
                    prev,
                    normalized.feedbackLanguages,
                    normalized.defaultFeedbackLanguage
                ))
            })
            .catch(() => {
                setStatusMessage('모델 메타를 불러오지 못했습니다. 서버 기본 설정으로 진행합니다.')
            })
        setStatusMessage('')
    }, [])

    const handleSaveApiKey = useCallback(async () => {
        await setApiKey(apiKeyInput)
        try {
            const result = await verifyApiKey(apiKeyInput, feedbackModel || modelMeta.defaultFeedbackModel)
            if (result.valid) {
                setStatusMessage('API Key 검증이 완료되었습니다.')
            } else {
                setStatusMessage(`API Key 검증 실패: ${result.message}`)
            }
        } catch (error) {
            setStatusMessage(`API Key 검증 실패: ${error.message}`)
        }
    }, [apiKeyInput, feedbackModel, modelMeta.defaultFeedbackModel])

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
        feedbackModel,
        ttsModel
    }

    const rulebookProps = {
        rulebooks,
        uploadRulebook,
        toggleRulebook,
        deleteRulebook
    }

    const questionGroupManagerProps = {
        activeGroupId,
        questionGroups,
        setActiveGroupId,
        isCreateGroupModalOpen,
        openCreateGroupModal,
        closeCreateGroupModal,
        newGroupName,
        setNewGroupName,
        newGroupTagsInput,
        setNewGroupTagsInput,
        createGroup,
        isEditGroupModalOpen,
        openEditGroupModal,
        closeEditGroupModal,
        editingGroupName,
        setEditingGroupName,
        editingGroupTagsInput,
        setEditingGroupTagsInput,
        deleteGroup,
        saveEditedGroup
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
        ttsModel,
        setTtsModel,
        feedbackModelOptions: modelMeta.feedbackModels.length > 0
            ? modelMeta.feedbackModels
            : (feedbackModel ? [feedbackModel] : []),
        voiceSttModelOptions: modelMeta.voiceSttModels.length > 0
            ? modelMeta.voiceSttModels
            : (voiceSttModel ? [voiceSttModel] : []),
        ttsModelOptions: modelMeta.ttsModels.length > 0
            ? modelMeta.ttsModels
            : (ttsModel ? [ttsModel] : []),
        apiKeyInput,
        setApiKeyInput,
        voice,
        setVoice,
        voiceOptions: modelMeta.voices.length > 0
            ? modelMeta.voices
            : (voice ? [voice] : []),
        feedbackLang,
        setFeedbackLang,
        feedbackLanguageOptions: modelMeta.feedbackLanguages,
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
            handleAudioQuickAction,
            replayButtonLabel,
            replayButtonDisabled,
            replayButtonDisabledReason,
            handleReplayAction
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
            ttsModel,
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
