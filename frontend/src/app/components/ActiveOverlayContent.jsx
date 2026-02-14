import React from 'react'
import {RulebookOverlay} from '../../features/rulebooks/components/RulebookOverlay.jsx'
import {QuestionGroupManagerOverlay} from '../../features/questions/components/QuestionGroupManagerOverlay.jsx'
import {GroupQuestionManagerOverlay} from '../../features/questions/components/GroupQuestionManagerOverlay.jsx'
import {LearningModeOverlay} from '../../features/questions/components/LearningModeOverlay.jsx'
import {WrongNotesOverlay} from '../../features/wrongnotes/components/WrongNotesOverlay.jsx'
import {ModelSettingsOverlay} from '../../features/settings/components/ModelSettingsOverlay.jsx'

export function ActiveOverlayContent({
                                         activePanel,
                                         rulebookProps,
                                         questionGroupManagerProps,
                                         groupQuestionManagerProps,
                                         learningModeProps,
                                         wrongNotesProps,
                                         modelSettingsProps
                                     }) {
    if (activePanel === 'rulebook') {
        return <RulebookOverlay {...rulebookProps}/>
    }

    if (activePanel === 'question-group-manager') {
        return <QuestionGroupManagerOverlay {...questionGroupManagerProps}/>
    }

    if (activePanel === 'group-question-manager') {
        return <GroupQuestionManagerOverlay {...groupQuestionManagerProps}/>
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
