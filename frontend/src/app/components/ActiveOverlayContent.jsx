import React from 'react'
import {RulebookOverlay} from '../../features/rulebooks/components/RulebookOverlay.jsx'
import {QuestionManagerOverlay} from '../../features/questions/components/QuestionManagerOverlay.jsx'
import {LearningModeOverlay} from '../../features/questions/components/LearningModeOverlay.jsx'
import {WrongNotesOverlay} from '../../features/wrongnotes/components/WrongNotesOverlay.jsx'
import {ModelSettingsOverlay} from '../../features/settings/components/ModelSettingsOverlay.jsx'

export function ActiveOverlayContent({
                                         activePanel,
                                         rulebookProps,
                                         questionManagerProps,
                                         learningModeProps,
                                         wrongNotesProps,
                                         modelSettingsProps
                                     }) {
    if (activePanel === 'rulebook') {
        return <RulebookOverlay {...rulebookProps}/>
    }

    if (activePanel === 'question-manager') {
        return <QuestionManagerOverlay {...questionManagerProps}/>
    }

    if (activePanel === 'learning-mode') {
        return <LearningModeOverlay {...learningModeProps}/>
    }

    if (activePanel === 'wrongnotes') {
        return <WrongNotesOverlay {...wrongNotesProps}/>
    }

    if (activePanel === 'model') {
        return <ModelSettingsOverlay {...modelSettingsProps}/>
    }

    return null
}
