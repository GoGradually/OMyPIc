import React from 'react'
import {Header} from '../features/layout/components/Header.jsx'
import {VoicePanel} from '../features/voice/components/VoicePanel.jsx'
import {QuestionPanel} from '../features/questions/components/QuestionPanel.jsx'
import {TranscriptPanel} from '../features/voice/components/TranscriptPanel.jsx'
import {SummaryPanel} from '../features/settings/components/SummaryPanel.jsx'
import {RecentFeedbackPanel} from '../features/wrongnotes/components/RecentFeedbackPanel.jsx'
import {StatusBar} from '../features/layout/components/StatusBar.jsx'
import {OverlayShell} from '../features/layout/components/OverlayShell.jsx'
import {ActiveOverlayContent} from './components/ActiveOverlayContent.jsx'
import {useAppViewModel} from './hooks/useAppViewModel.js'

export default function App() {
    const {
        activePanel,
        setActivePanel,
        togglePanel,
        overlayTitle,
        overlayContentProps,
        voicePanelProps,
        questionPanelProps,
        transcriptPanelProps,
        summaryPanelProps,
        recentFeedbackPanelProps,
        statusBarProps
    } = useAppViewModel()

    return (
        <div className="app-shell app">
            <Header activePanel={activePanel} onTogglePanel={togglePanel}/>

            <div className="workspace app__workspace">
                <main className="practice-column app__practice-column">
                    <VoicePanel {...voicePanelProps} />
                    <QuestionPanel {...questionPanelProps} />
                    <TranscriptPanel {...transcriptPanelProps} />
                </main>

                <aside className="info-column app__info-column">
                    <SummaryPanel {...summaryPanelProps} />
                    <RecentFeedbackPanel {...recentFeedbackPanelProps} />
                </aside>
            </div>

            <StatusBar {...statusBarProps} />

            {activePanel && (
                <OverlayShell title={overlayTitle} onClose={() => setActivePanel('')}>
                    <ActiveOverlayContent {...overlayContentProps} />
                </OverlayShell>
            )}
        </div>
    )
}
