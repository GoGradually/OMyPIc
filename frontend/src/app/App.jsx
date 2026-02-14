import React, {useCallback, useEffect, useState} from 'react'
import {getApiKey, setApiKey, verifyApiKey} from '../shared/api/http.js'
import {FEEDBACK_MODELS, getModeSummary, PANEL_TITLES, VOICE_STT_MODELS, VOICES} from '../shared/constants/models.js'
import {copyText} from '../shared/utils/clipboard.js'
import {getCurrentQuestionLabel} from '../shared/utils/mode.js'
import {useSessionId} from './providers/session.js'

import {useVoiceSession} from '../features/voice/hooks/useVoiceSession.js'
import {useRulebooks} from '../features/rulebooks/hooks/useRulebooks.js'
import {useWrongNotes} from '../features/wrongnotes/hooks/useWrongNotes.js'
import {useQuestionGroups} from '../features/questions/hooks/useQuestionGroups.js'

import {getAudioUiState} from '../features/voice/utils/audioStatus.js'

import {Header} from '../features/layout/components/Header.jsx'
import {VoicePanel} from '../features/voice/components/VoicePanel.jsx'
import {QuestionPanel} from '../features/questions/components/QuestionPanel.jsx'
import {TranscriptPanel} from '../features/voice/components/TranscriptPanel.jsx'
import {SummaryPanel} from '../features/settings/components/SummaryPanel.jsx'
import {RecentFeedbackPanel} from '../features/wrongnotes/components/RecentFeedbackPanel.jsx'
import {StatusBar} from '../features/layout/components/StatusBar.jsx'
import {OverlayShell} from '../features/layout/components/OverlayShell.jsx'
import {RulebookOverlay} from '../features/rulebooks/components/RulebookOverlay.jsx'
import {QuestionManagerOverlay} from '../features/questions/components/QuestionManagerOverlay.jsx'
import {LearningModeOverlay} from '../features/questions/components/LearningModeOverlay.jsx'
import {WrongNotesOverlay} from '../features/wrongnotes/components/WrongNotesOverlay.jsx'
import {ModelSettingsOverlay} from '../features/settings/components/ModelSettingsOverlay.jsx'

export default function App() {
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

    if (activePanel === 'question-manager') {
        overlayContent = (
            <QuestionManagerOverlay
                activeGroupId={activeGroupId}
                newGroupName={newGroupName}
                setNewGroupName={setNewGroupName}
                newGroupTagsInput={newGroupTagsInput}
                setNewGroupTagsInput={setNewGroupTagsInput}
                questionGroups={questionGroups}
                setActiveGroupId={setActiveGroupId}
                createGroup={createGroup}
                deleteGroup={deleteGroup}
                newQuestion={newQuestion}
                setNewQuestion={setNewQuestion}
                newQuestionType={newQuestionType}
                setNewQuestionType={setNewQuestionType}
                addQuestion={addQuestion}
                activeQuestionGroup={activeQuestionGroup}
                editingQuestionId={editingQuestionId}
                editingQuestionText={editingQuestionText}
                setEditingQuestionText={setEditingQuestionText}
                editingQuestionType={editingQuestionType}
                setEditingQuestionType={setEditingQuestionType}
                startEditQuestion={startEditQuestion}
                saveEditedQuestion={saveEditedQuestion}
                cancelEditQuestion={cancelEditQuestion}
                removeQuestion={removeQuestion}
            />
        )
    }

    if (activePanel === 'learning-mode') {
        overlayContent = (
            <LearningModeOverlay
                mode={mode}
                setMode={setMode}
                batchSize={batchSize}
                setBatchSize={setBatchSize}
                updateMode={updateMode}
                tagStats={tagStats}
                selectedGroupTags={selectedGroupTags}
                toggleSelectedTag={toggleSelectedTag}
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
                voiceSttModel={voiceSttModel}
                setVoiceSttModel={setVoiceSttModel}
                feedbackModel={feedbackModel}
                setFeedbackModel={setFeedbackModel}
                apiKeyInput={apiKeyInput}
                setApiKeyInput={setApiKeyInput}
                voice={voice}
                setVoice={setVoice}
                feedbackLang={feedbackLang}
                setFeedbackLang={setFeedbackLang}
                onSaveApiKey={handleSaveApiKey}
            />
        )
    }

    return (
        <div className="app-shell app">
            <Header activePanel={activePanel} onTogglePanel={togglePanel}/>

            <div className="workspace app__workspace">
                <main className="practice-column app__practice-column">
                    <VoicePanel
                        voiceConnected={voiceConnected}
                        sessionActive={sessionActive}
                        speechState={speechState}
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
                        onOpenQuestionManager={() => setActivePanel('question-manager')}
                        onOpenLearningMode={() => setActivePanel('learning-mode')}
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
                        voiceSttModel={voiceSttModel}
                        feedbackModel={feedbackModel}
                        enabledRulebookCount={enabledRulebookCount}
                        questionGroupCount={questionGroups.length}
                        onOpenSettings={() => setActivePanel('model')}
                    />

                    <RecentFeedbackPanel
                        feedback={feedback}
                        onOpenWrongnotes={() => setActivePanel('wrongnotes')}
                    />
                </aside>
            </div>

            <StatusBar
                voiceConnected={voiceConnected}
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
