/** @vitest-environment jsdom */
import React from 'react'
import {describe, expect, it, vi} from 'vitest'
import {render, screen} from '@testing-library/react'
import {ActiveOverlayContent} from './ActiveOverlayContent.jsx'

vi.mock('../../features/rulebooks/components/RulebookOverlay.jsx', () => ({
    RulebookOverlay: () => <div>rulebook-overlay</div>
}))

vi.mock('../../features/questions/components/QuestionGroupManagerOverlay.jsx', () => ({
    QuestionGroupManagerOverlay: () => <div>question-group-manager-overlay</div>
}))

vi.mock('../../features/questions/components/GroupQuestionManagerOverlay.jsx', () => ({
    GroupQuestionManagerOverlay: () => <div>group-question-manager-overlay</div>
}))

vi.mock('../../features/questions/components/LearningModeOverlay.jsx', () => ({
    LearningModeOverlay: () => <div>learning-mode-overlay</div>
}))

vi.mock('../../features/wrongnotes/components/WrongNotesOverlay.jsx', () => ({
    WrongNotesOverlay: () => <div>wrongnotes-overlay</div>
}))

vi.mock('../../features/settings/components/ModelSettingsOverlay.jsx', () => ({
    ModelSettingsOverlay: () => <div>model-overlay</div>
}))

function renderPanel(activePanel) {
    render(
        <ActiveOverlayContent
            activePanel={activePanel}
            rulebookProps={{}}
            questionGroupManagerProps={{}}
            groupQuestionManagerProps={{}}
            learningModeProps={{}}
            wrongNotesProps={{}}
            modelSettingsProps={{}}
        />
    )
}

describe('ActiveOverlayContent', () => {
    it('renders question group manager overlay for question-group-manager panel', () => {
        renderPanel('question-group-manager')
        expect(screen.getByText('question-group-manager-overlay')).toBeTruthy()
    })

    it('renders group question manager overlay for group-question-manager panel', () => {
        renderPanel('group-question-manager')
        expect(screen.getByText('group-question-manager-overlay')).toBeTruthy()
    })
})
