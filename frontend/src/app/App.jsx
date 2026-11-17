import React, {useCallback} from 'react'
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

    const closeOverlay = useCallback(() => setActivePanel(''), [setActivePanel])

    return (
        <div className="app-shell">
            <Header activePanel={activePanel} onTogglePanel={togglePanel}/>

            <div className="workspace">
                <main className="sheet">
                    <VoicePanel {...voicePanelProps} />
                    <QuestionPanel {...questionPanelProps} />
                    <TranscriptPanel {...transcriptPanelProps} />
                </main>

                <aside className="side">
                    <RecentFeedbackPanel {...recentFeedbackPanelProps} />
                    <SummaryPanel {...summaryPanelProps} />
                </aside>
            </div>

            <StatusBar {...statusBarProps} />

            {activePanel && (
                <OverlayShell title={overlayTitle} onClose={closeOverlay}>
                    <ActiveOverlayContent {...overlayContentProps} />
                </OverlayShell>
            )}
        </div>
    )
}
