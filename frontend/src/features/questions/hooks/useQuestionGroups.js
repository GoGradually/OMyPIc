import {useCallback, useState} from 'react'
import {useQuestionManager} from './useQuestionManager.js'
import {useLearningMode} from './useLearningMode.js'

export function useQuestionGroups({
                                      sessionId,
                                      onStatus
                                  }) {
    const [currentQuestion, setCurrentQuestion] = useState(null)

    const learningMode = useLearningMode({sessionId, onStatus})
    const questionManager = useQuestionManager({
        onGroupsRefreshed: learningMode.refreshTagStats
    })

    const deleteGroup = useCallback(async (groupId) => {
        await questionManager.deleteGroup(groupId)
        setCurrentQuestion(null)
    }, [questionManager.deleteGroup])

    return {
        ...questionManager,
        ...learningMode,
        deleteGroup,
        currentQuestion,
        setCurrentQuestion
    }
}
