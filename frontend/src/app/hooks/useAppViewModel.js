import {useCallback, useEffect, useState} from 'react'
import {
    ANTHROPIC_PROVIDER,
    exportDataBundle,
    getApiKey,
    getModelMeta,
    importDataBundle,
    OPENAI_PROVIDER,
    setApiKey,
    verifyApiKey
} from '../../shared/api/http.js'
import {
    DEVICE_TTS_MODEL,
    getFeedbackProvider,
    getModelLabel,
    getModeSummary,
    getProviderLabel,
    LOCAL_STT_MODEL,
    LOCAL_TTS_MODEL,
    PANEL_TITLES
} from '../../shared/constants/models.js'
import {readPreference, writePreference} from '../../shared/utils/preferences.js'
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
    feedbackProviders: [],
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
        feedbackProviders: normalizeFeedbackProviders(meta.feedbackProviders, feedbackModels),
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

// Older backends only send the flat feedbackModels list; treat it as a single OpenAI group.
function normalizeFeedbackProviders(groups, feedbackModels) {
    const normalized = (Array.isArray(groups) ? groups : [])
        .filter((group) => group && typeof group.provider === 'string' && Array.isArray(group.models))
        .map((group) => ({
            provider: group.provider,
            models: group.models.filter(Boolean),
            defaultModel: typeof group.defaultModel === 'string' ? group.defaultModel : ''
        }))
        .filter((group) => group.models.length > 0)
    if (normalized.length > 0) {
        return normalized
    }
    return feedbackModels.length > 0
        ? [{provider: OPENAI_PROVIDER, models: feedbackModels, defaultModel: feedbackModels[0]}]
        : []
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

function buildBackupFilename() {
    const now = new Date()
    const pad2 = (value) => String(value).padStart(2, '0')
    const datePart = `${now.getFullYear()}${pad2(now.getMonth() + 1)}${pad2(now.getDate())}`
    const timePart = `${pad2(now.getHours())}${pad2(now.getMinutes())}${pad2(now.getSeconds())}`
    return `omypic-backup-${datePart}-${timePart}.zip`
}

function triggerBlobDownload(blob, filename) {
    const objectUrl = URL.createObjectURL(blob)
    const anchor = document.createElement('a')
    anchor.href = objectUrl
    anchor.download = filename
    document.body.appendChild(anchor)
    anchor.click()
    document.body.removeChild(anchor)
    URL.revokeObjectURL(objectUrl)
}

function pickFileFromInput() {
    return new Promise((resolve) => {
        const input = document.createElement('input')
        input.type = 'file'
        input.accept = '.zip,application/zip'
        input.onchange = () => {
            const selected = input.files && input.files[0] ? input.files[0] : null
            resolve(selected)
        }
        input.click()
    })
}

export function useAppViewModel() {
    const sessionId = useSessionId()

    const [feedbackModel, setFeedbackModel] = useState(() => readPreference('feedback_model', ''))
    const [aiEnabled, setAiEnabled] = useState(() => readPreference('ai_enabled', true) !== false)
    const [autoReplayEnabled, setAutoReplayEnabled] = useState(() => readPreference('auto_replay', true) !== false)
    const [voiceSttModel, setVoiceSttModel] = useState(() => readPreference('stt_model', ''))
    const [ttsModel, setTtsModel] = useState(() => readPreference('tts_model', ''))
    const [modelMeta, setModelMeta] = useState(EMPTY_MODEL_META)

    const [apiKeyInput, setApiKeyInput] = useState('')
    const [claudeKeyInput, setClaudeKeyInput] = useState('')
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

    const feedbackProvider = getFeedbackProvider(feedbackModel, modelMeta.feedbackProviders)

    useEffect(() => {
        if (feedbackModel) {
            writePreference('feedback_model', feedbackModel)
        }
    }, [feedbackModel])

    useEffect(() => {
        if (voiceSttModel) {
            writePreference('stt_model', voiceSttModel)
        }
    }, [voiceSttModel])

    useEffect(() => {
        if (ttsModel) {
            writePreference('tts_model', ttsModel)
        }
    }, [ttsModel])

    useEffect(() => {
        writePreference('ai_enabled', aiEnabled)
    }, [aiEnabled])

    useEffect(() => {
        writePreference('auto_replay', autoReplayEnabled)
    }, [autoReplayEnabled])

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
        handleReplayAction,
        replayPlaying,
        sessionAiEnabled,
        awaitingReview
    } = useVoiceSession({
        sessionId,
        feedbackModel,
        feedbackProvider,
        aiEnabled,
        autoReplay: autoReplayEnabled,
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
        getApiKey(OPENAI_PROVIDER).then((key) => setApiKeyInput(key || ''))
        getApiKey(ANTHROPIC_PROVIDER).then((key) => setClaudeKeyInput(key || ''))
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

    const defaultModelOf = useCallback((provider) => {
        const group = modelMeta.feedbackProviders.find((item) => item.provider === provider)
        return group?.defaultModel || group?.models?.[0] || ''
    }, [modelMeta.feedbackProviders])

    const saveAndVerifyKey = useCallback(async (provider, key) => {
        const label = getProviderLabel(provider)
        const trimmedKey = (key || '').trim()
        try {
            await setApiKey(trimmedKey, provider)
        } catch (error) {
            // e.g. the OS keychain is unavailable: tell the learner instead of failing silently.
            return `${label} API Key 저장 실패: ${error.message}`
        }
        if (!trimmedKey) {
            return `${label} API Key를 비웠습니다.`
        }
        // Verify against a model of the same provider: the selected one if it matches, else that provider's default.
        const model = provider === feedbackProvider ? feedbackModel : defaultModelOf(provider)
        try {
            const result = await verifyApiKey(trimmedKey, model, provider)
            return result.valid
                ? `${label} API Key 검증 완료.`
                : `${label} API Key 검증 실패: ${result.message}`
        } catch (error) {
            return `${label} API Key 검증 실패: ${error.message}`
        }
    }, [defaultModelOf, feedbackModel, feedbackProvider])

    const handleSaveApiKey = useCallback(async () => {
        const messages = [await saveAndVerifyKey(OPENAI_PROVIDER, apiKeyInput)]
        const claudeKeyNeeded = feedbackProvider === ANTHROPIC_PROVIDER
        if (claudeKeyInput.trim() || claudeKeyNeeded) {
            messages.push(await saveAndVerifyKey(ANTHROPIC_PROVIDER, claudeKeyInput))
        } else {
            await setApiKey('', ANTHROPIC_PROVIDER).catch((error) => {
                messages.push(`${getProviderLabel(ANTHROPIC_PROVIDER)} API Key 삭제 실패: ${error.message}`)
            })
        }
        setStatusMessage(messages.join(' '))
    }, [apiKeyInput, claudeKeyInput, feedbackProvider, saveAndVerifyKey])

    const handleExportData = useCallback(async () => {
        try {
            const backupBlob = await exportDataBundle()
            const filename = buildBackupFilename()
            if (window.omypic && window.omypic.saveBackupFile) {
                const bytes = new Uint8Array(await backupBlob.arrayBuffer())
                const saved = await window.omypic.saveBackupFile(filename, bytes)
                if (!saved) {
                    setStatusMessage('데이터 내보내기를 취소했습니다.')
                    return
                }
            } else {
                triggerBlobDownload(backupBlob, filename)
            }
            setStatusMessage('데이터 내보내기를 완료했습니다.')
        } catch (error) {
            setStatusMessage(`데이터 내보내기에 실패했습니다: ${error.message}`)
        }
    }, [])

    const handleImportData = useCallback(async () => {
        const confirmed = window.confirm('데이터 가져오기를 진행하면 현재 저장 데이터가 전체 교체됩니다. 계속하시겠습니까?')
        if (!confirmed) {
            return
        }

        try {
            let file = null
            if (window.omypic && window.omypic.pickBackupFile) {
                const selected = await window.omypic.pickBackupFile()
                if (!selected || !selected.bytes) {
                    setStatusMessage('데이터 가져오기를 취소했습니다.')
                    return
                }
                file = new File([new Uint8Array(selected.bytes)], selected.name || 'omypic-backup.zip', {
                    type: 'application/zip'
                })
            } else {
                file = await pickFileFromInput()
                if (!file) {
                    setStatusMessage('데이터 가져오기를 취소했습니다.')
                    return
                }
            }

            const result = await importDataBundle(file)
            await Promise.all([
                refreshRulebooks(),
                refreshQuestionGroups(),
                refreshWrongNotes()
            ])
            setStatusMessage(
                `데이터 가져오기를 완료했습니다. 질문 그룹 ${result.questionGroupCount}개, 룰북 ${result.rulebookCount}개가 반영되었습니다. 앱을 재시작하면 안정적으로 반영됩니다.`
            )
        } catch (error) {
            setStatusMessage(`데이터 가져오기에 실패했습니다: ${error.message}`)
        }
    }, [refreshQuestionGroups, refreshRulebooks, refreshWrongNotes])

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

    const usesLocalStt = voiceSttModel === LOCAL_STT_MODEL
    const usesLocalSpeech = !aiEnabled || ttsModel === DEVICE_TTS_MODEL || ttsModel === LOCAL_TTS_MODEL
    const needsOpenAiKey = aiEnabled
        && (!usesLocalStt || !usesLocalSpeech || feedbackProvider === OPENAI_PROVIDER)

    const handleStartSession = useCallback(async () => {
        try {
            // AI off calls no provider. With AI on, the OpenAI key is only needed for what actually
            // runs on OpenAI: hosted STT, hosted TTS, or a GPT feedback model.
            if (needsOpenAiKey && !(await getApiKey(OPENAI_PROVIDER))) {
                setStatusMessage('선택한 설정에는 OpenAI API Key가 필요합니다. 키를 저장하거나, STT/TTS를 무료 로컬 모델로 바꾸고 Claude 피드백 모델을 선택해 주세요.')
                setActivePanel('model')
                return
            }
            const needsClaudeKey = aiEnabled && feedbackProvider === ANTHROPIC_PROVIDER
            if (needsClaudeKey && !(await getApiKey(ANTHROPIC_PROVIDER))) {
                setStatusMessage('Claude 모델로 피드백을 받으려면 설정에서 Claude API Key를 저장해 주세요.')
                setActivePanel('model')
                return
            }
            await updateMode()
            await startSession()
        } catch (error) {
            setStatusMessage(error?.message || '세션 시작에 실패했습니다.')
        }
    }, [aiEnabled, feedbackProvider, needsOpenAiKey, updateMode, startSession])

    const handleStopSession = useCallback(async () => {
        await stopSession({forced: true, reason: 'user_stop', statusMessage: '세션을 종료했습니다.'})
    }, [stopSession])

    const currentQuestionLabel = getCurrentQuestionLabel(currentQuestion)
    // With AI off the learning mode's feedback timing does not apply.
    const modeSummary = aiEnabled
        ? getModeSummary(mode, batchSize)
        : '질문과 답변만 진행 (AI 꺼짐)'
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

    // During a session the stage follows what that session was opened with, not the toggle.
    const stageAiEnabled = sessionActive ? sessionAiEnabled : aiEnabled
    const feedbackSummary = aiEnabled ? getModelLabel(feedbackModel) : '꺼짐'

    // AI off still reads questions aloud: with the selected local model, or else the device voice.
    const localSpeechModel = aiEnabled || ttsModel === LOCAL_TTS_MODEL ? ttsModel : DEVICE_TTS_MODEL

    const statusDetails = {
        sessionIdPrefix: sessionId.slice(0, 8),
        audioPermissionLabel,
        audioInputCount: audioDeviceStatus.inputCount,
        voiceSttModel: aiEnabled ? voiceSttModel : '꺼짐',
        feedbackModel: aiEnabled ? feedbackModel : '꺼짐',
        ttsModel: localSpeechModel
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
        feedbackProviderGroups: modelMeta.feedbackProviders,
        feedbackProvider,
        claudeKeyInput,
        setClaudeKeyInput,
        aiEnabled,
        setAiEnabled,
        autoReplayEnabled,
        setAutoReplayEnabled,
        needsOpenAiKey,
        sessionActive,
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
        onSaveApiKey: handleSaveApiKey,
        onExportData: handleExportData,
        onImportData: handleImportData
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
            handleReplayAction,
            replayPlaying,
            awaitingReview,
            currentQuestionLabel,
            hasQuestion: Boolean(currentQuestion?.text),
            stageAiEnabled,
            aiEnabled,
            onToggleAi: setAiEnabled,
            autoReplayEnabled,
            onToggleAutoReplay: setAutoReplayEnabled,
            feedbackSummary,
            continuousMode: mode === 'CONTINUOUS'
        },
        questionPanelProps: {
            currentQuestionLabel,
            modeSummary,
            onOpenQuestionGroupManager: () => setActivePanel('question-group-manager'),
            onOpenGroupQuestionManager: () => setActivePanel('group-question-manager'),
            onOpenLearningMode: () => setActivePanel('learning-mode')
        },
        transcriptPanelProps: {
            aiEnabled: stageAiEnabled,
            userText,
            transcript,
            onCopyUserText: copyUserText
        },
        summaryPanelProps: {
            feedbackLang,
            voice,
            voiceSttModel,
            feedbackModel,
            aiEnabled,
            ttsModel,
            enabledRulebookCount,
            questionGroupCount: questionGroups.length,
            onOpenSettings: () => setActivePanel('model')
        },
        recentFeedbackPanelProps: {
            feedback,
            aiEnabled,
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
