import React, {useCallback, useEffect, useState} from 'react'
import {getApiKey, setApiKey, verifyApiKey} from '../shared/api/http.js'
import {
    FEEDBACK_MODELS,
    getModeSummary,
    MOCK_FINAL_MODELS,
    PANEL_TITLES,
    REALTIME_CONVERSATION_MODELS,
    REALTIME_STT_MODELS,
    VOICES
} from '../shared/constants/models.js'
import {copyText} from '../shared/utils/clipboard.js'
import {getCurrentQuestionLabel} from '../shared/utils/mode.js'
import {useSessionId} from './providers/session.js'

import {useRealtimeSession} from '../features/realtime/hooks/useRealtimeSession.js'
import {useRulebooks} from '../features/rulebooks/hooks/useRulebooks.js'
import {useWrongNotes} from '../features/wrongnotes/hooks/useWrongNotes.js'
import {useQuestionLists} from '../features/questions/hooks/useQuestionLists.js'

import {getAudioUiState} from '../features/realtime/utils/audioStatus.js'

import {Header} from '../features/layout/components/Header.jsx'
import {VoicePanel} from '../features/realtime/components/VoicePanel.jsx'
import {QuestionPanel} from '../features/questions/components/QuestionPanel.jsx'
import {TranscriptPanel} from '../features/realtime/components/TranscriptPanel.jsx'
import {SummaryPanel} from '../features/settings/components/SummaryPanel.jsx'
import {RecentFeedbackPanel} from '../features/wrongnotes/components/RecentFeedbackPanel.jsx'
import {StatusBar} from '../features/layout/components/StatusBar.jsx'
import {OverlayShell} from '../features/layout/components/OverlayShell.jsx'
import {RulebookOverlay} from '../features/rulebooks/components/RulebookOverlay.jsx'
import {QuestionsOverlay} from '../features/questions/components/QuestionsOverlay.jsx'
import {WrongNotesOverlay} from '../features/wrongnotes/components/WrongNotesOverlay.jsx'
import {ModelSettingsOverlay} from '../features/settings/components/ModelSettingsOverlay.jsx'

export default function App() {
    const sessionId = useSessionId()

    const [provider, setProvider] = useState('openai')
    const [feedbackModel, setFeedbackModel] = useState(FEEDBACK_MODELS.openai[0])
    const [realtimeConversationModel, setRealtimeConversationModel] = useState(REALTIME_CONVERSATION_MODELS[0])
    const [realtimeSttModel, setRealtimeSttModel] = useState(REALTIME_STT_MODELS[0])
    const [mockFinalModel, setMockFinalModel] = useState(MOCK_FINAL_MODELS.openai[0])

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
        questionLists,
        activeListId,
        setActiveListId,
        activeQuestionList,
        newListName,
        setNewListName,
        newQuestion,
        setNewQuestion,
        newGroup,
        setNewGroup,
        editingQuestionId,
        editingQuestionText,
        setEditingQuestionText,
        editingQuestionGroup,
        setEditingQuestionGroup,
        mode,
        setMode,
        batchSize,
        setBatchSize,
        mockOrder,
        setMockOrder,
        mockCounts,
        setMockCounts,
        currentQuestion,
        refreshQuestionLists,
        createList,
        deleteList,
        addQuestion,
        startEditQuestion,
        cancelEditQuestion,
        saveEditedQuestion,
        removeQuestion,
        nextQuestion,
        updateMode,
        setCurrentQuestion
    } = useQuestionLists({
        sessionId,
        provider,
        mockFinalModel,
        feedbackLang,
        onFeedback: setFeedback,
        onStatus: setStatusMessage,
        refreshWrongNotes
    })

    const {
        sessionActive,
        realtimeConnected,
        partialTranscript,
        transcript,
        userText,
        audioPermission,
        audioDeviceStatus,
        startSession,
        stopSession,
        syncRealtimeSettings,
        handleAudioQuickAction
    } = useRealtimeSession({
        sessionId,
        provider,
        feedbackModel,
        realtimeConversationModel,
        realtimeSttModel,
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
        refreshQuestionLists().catch(() => {
        })
        refreshWrongNotes().catch(() => {
        })
    }, [refreshRulebooks, refreshQuestionLists, refreshWrongNotes])

    useEffect(() => {
        setFeedbackModel(FEEDBACK_MODELS[provider][0])
        setMockFinalModel(MOCK_FINAL_MODELS[provider][0])
        getApiKey(provider).then((key) => setApiKeyInput(key || ''))
        setStatusMessage('')
    }, [provider])

    const handleSaveApiKey = useCallback(async () => {
        await setApiKey(provider, apiKeyInput)
        try {
            const result = await verifyApiKey(provider, apiKeyInput, feedbackModel)
            if (result.valid) {
                setStatusMessage('API Key 검증이 완료되었습니다.')
            } else {
                setStatusMessage(`API Key 검증 실패: ${result.message}`)
            }
        } catch (error) {
            setStatusMessage(`API Key 검증 실패: ${error.message}`)
        }
    }, [provider, apiKeyInput, feedbackModel])

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
        if (!activeListId) {
            setStatusMessage('질문 리스트를 선택하고 학습 모드를 먼저 적용해 주세요.')
            return
        }
        try {
            await updateMode()
            await startSession()
        } catch (error) {
            setStatusMessage(error?.message || '세션 시작에 실패했습니다.')
        }
    }, [activeListId, updateMode, startSession])

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
        provider,
        audioPermissionLabel,
        audioInputCount: audioDeviceStatus.inputCount,
        realtimeConversationModel,
        realtimeSttModel,
        feedbackModel,
        mockFinalModel
    }

    let overlayContent = null
    if (activePanel === 'rulebook') {
        overlayContent = (
            <RulebookOverlay
                rulebooks={rulebooks}
                uploadRulebook={uploadRulebook}
                toggleRulebook={toggleRulebook}
                deleteRulebook={deleteRulebook}
            />
        )
    }

    if (activePanel === 'questions') {
        overlayContent = (
            <QuestionsOverlay
                mode={mode}
                setMode={setMode}
                batchSize={batchSize}
                setBatchSize={setBatchSize}
                mockOrder={mockOrder}
                setMockOrder={setMockOrder}
                mockCounts={mockCounts}
                setMockCounts={setMockCounts}
                updateMode={updateMode}
                nextQuestion={nextQuestion}
                activeListId={activeListId}
                newListName={newListName}
                setNewListName={setNewListName}
                questionLists={questionLists}
                setActiveListId={setActiveListId}
                createList={createList}
                deleteList={deleteList}
                newQuestion={newQuestion}
                setNewQuestion={setNewQuestion}
                newGroup={newGroup}
                setNewGroup={setNewGroup}
                addQuestion={addQuestion}
                activeQuestionList={activeQuestionList}
                editingQuestionId={editingQuestionId}
                editingQuestionText={editingQuestionText}
                setEditingQuestionText={setEditingQuestionText}
                editingQuestionGroup={editingQuestionGroup}
                setEditingQuestionGroup={setEditingQuestionGroup}
                startEditQuestion={startEditQuestion}
                saveEditedQuestion={saveEditedQuestion}
                cancelEditQuestion={cancelEditQuestion}
                removeQuestion={removeQuestion}
            />
        )
    }

    if (activePanel === 'wrongnotes') {
        overlayContent = (
            <WrongNotesOverlay
                wrongNotes={wrongNotes}
                feedback={feedback}
            />
        )
    }

    if (activePanel === 'model') {
        overlayContent = (
            <ModelSettingsOverlay
                provider={provider}
                setProvider={setProvider}
                realtimeConversationModel={realtimeConversationModel}
                setRealtimeConversationModel={setRealtimeConversationModel}
                realtimeSttModel={realtimeSttModel}
                setRealtimeSttModel={setRealtimeSttModel}
                feedbackModel={feedbackModel}
                setFeedbackModel={setFeedbackModel}
                mockFinalModel={mockFinalModel}
                setMockFinalModel={setMockFinalModel}
                apiKeyInput={apiKeyInput}
                setApiKeyInput={setApiKeyInput}
                voice={voice}
                setVoice={setVoice}
                feedbackLang={feedbackLang}
                setFeedbackLang={setFeedbackLang}
                onSaveApiKey={handleSaveApiKey}
                onSyncRealtimeSettings={syncRealtimeSettings}
            />
        )
    }

    return (
        <div className="app-shell app">
            <Header activePanel={activePanel} onTogglePanel={togglePanel}/>

            <div className="workspace app__workspace">
                <main className="practice-column app__practice-column">
                    <VoicePanel
                        realtimeConnected={realtimeConnected}
                        sessionActive={sessionActive}
                        partialTranscript={partialTranscript}
                        startSession={handleStartSession}
                        stopSession={handleStopSession}
                        audioConnectionReady={audioConnectionReady}
                        audioConnectionLabel={audioConnectionLabel}
                        audioPermissionLabel={audioPermissionLabel}
                        audioQuickHint={audioQuickHint}
                        audioPermission={audioPermission}
                        audioDeviceStatus={audioDeviceStatus}
                        handleAudioQuickAction={handleAudioQuickAction}
                    />

                    <QuestionPanel
                        currentQuestionLabel={currentQuestionLabel}
                        modeSummary={modeSummary}
                        onOpenQuestionsPanel={() => setActivePanel('questions')}
                    />

                    <TranscriptPanel
                        userText={userText}
                        transcript={transcript}
                        onCopyUserText={copyUserText}
                    />
                </main>

                <aside className="info-column app__info-column">
                    <SummaryPanel
                        feedbackLang={feedbackLang}
                        voice={voice}
                        realtimeConversationModel={realtimeConversationModel}
                        realtimeSttModel={realtimeSttModel}
                        feedbackModel={feedbackModel}
                        mockFinalModel={mockFinalModel}
                        enabledRulebookCount={enabledRulebookCount}
                        questionListCount={questionLists.length}
                        onOpenSettings={() => setActivePanel('model')}
                    />

                    <RecentFeedbackPanel
                        feedback={feedback}
                        onOpenWrongnotes={() => setActivePanel('wrongnotes')}
                    />
                </aside>
            </div>

            <StatusBar
                realtimeConnected={realtimeConnected}
                audioConnectionLabel={audioConnectionLabel}
                statusMessage={statusMessage}
                showStatusDetails={showStatusDetails}
                onToggleStatusDetails={() => setShowStatusDetails((prev) => !prev)}
                statusDetails={statusDetails}
            />

            {activePanel && (
                <OverlayShell title={PANEL_TITLES[activePanel]} onClose={() => setActivePanel('')}>
                    {overlayContent}
                </OverlayShell>
            )}
        </div>
    )
}
