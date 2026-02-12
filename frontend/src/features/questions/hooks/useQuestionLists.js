import {useCallback, useEffect, useMemo, useState} from 'react'
import {callApi} from '../../../shared/api/http.js'
import {buildModePayload} from '../../../shared/utils/mode.js'

export function useQuestionLists({
                                     sessionId,
                                     onStatus
                                 }) {
    const [questionLists, setQuestionLists] = useState([])
    const [activeListId, setActiveListId] = useState('')
    const [newListName, setNewListName] = useState('')
    const [newQuestion, setNewQuestion] = useState('')
    const [newGroup, setNewGroup] = useState('')

    const [editingQuestionId, setEditingQuestionId] = useState('')
    const [editingQuestionText, setEditingQuestionText] = useState('')
    const [editingQuestionGroup, setEditingQuestionGroup] = useState('')

    const [mode, setMode] = useState('IMMEDIATE')
    const [batchSize, setBatchSize] = useState(3)

    const [currentQuestion, setCurrentQuestion] = useState(null)

    const activeQuestionList = useMemo(
        () => questionLists.find((list) => list.id === activeListId) || null,
        [questionLists, activeListId]
    )

    const cancelEditQuestion = useCallback(() => {
        setEditingQuestionId('')
        setEditingQuestionText('')
        setEditingQuestionGroup('')
    }, [])

    const refreshQuestionLists = useCallback(async () => {
        const response = await callApi('/api/questions')
        const data = await response.json()
        setQuestionLists(data)
        setActiveListId((prev) => {
            if (!data.length) {
                return ''
            }
            if (prev && data.some((list) => list.id === prev)) {
                return prev
            }
            return data[0].id
        })
    }, [])

    const createList = useCallback(async () => {
        if (!newListName.trim()) {
            return
        }
        await callApi('/api/questions', {
            method: 'POST',
            headers: {'Content-Type': 'application/json'},
            body: JSON.stringify({name: newListName})
        })
        setNewListName('')
        await refreshQuestionLists()
    }, [newListName, refreshQuestionLists])

    const deleteList = useCallback(async () => {
        if (!activeListId) {
            return
        }
        await callApi(`/api/questions/${activeListId}`, {method: 'DELETE'})
        setCurrentQuestion(null)
        await refreshQuestionLists()
    }, [activeListId, refreshQuestionLists])

    const addQuestion = useCallback(async () => {
        if (!activeListId || !newQuestion.trim()) {
            return
        }
        await callApi(`/api/questions/${activeListId}/items`, {
            method: 'POST',
            headers: {'Content-Type': 'application/json'},
            body: JSON.stringify({text: newQuestion, group: newGroup || null})
        })
        setNewQuestion('')
        setNewGroup('')
        await refreshQuestionLists()
    }, [activeListId, newQuestion, newGroup, refreshQuestionLists])

    const startEditQuestion = useCallback((item) => {
        setEditingQuestionId(item.id)
        setEditingQuestionText(item.text || '')
        setEditingQuestionGroup(item.group || '')
    }, [])

    const saveEditedQuestion = useCallback(async () => {
        if (!activeListId || !editingQuestionId || !editingQuestionText.trim()) {
            return
        }
        await callApi(`/api/questions/${activeListId}/items/${editingQuestionId}`, {
            method: 'PUT',
            headers: {'Content-Type': 'application/json'},
            body: JSON.stringify({text: editingQuestionText, group: editingQuestionGroup || null})
        })
        cancelEditQuestion()
        await refreshQuestionLists()
    }, [
        activeListId,
        editingQuestionId,
        editingQuestionText,
        editingQuestionGroup,
        cancelEditQuestion,
        refreshQuestionLists
    ])

    const removeQuestion = useCallback(async (itemId) => {
        if (!activeListId || !itemId) {
            return
        }
        await callApi(`/api/questions/${activeListId}/items/${itemId}`, {method: 'DELETE'})
        if (editingQuestionId === itemId) {
            cancelEditQuestion()
        }
        await refreshQuestionLists()
    }, [activeListId, editingQuestionId, cancelEditQuestion, refreshQuestionLists])

    const nextQuestion = useCallback(async () => {
        if (!activeListId) {
            return
        }
        const response = await callApi(`/api/questions/${activeListId}/next?sessionId=${sessionId}`)
        const data = await response.json()
        setCurrentQuestion({
            ...data,
            exhausted: Boolean(data?.skipped),
            selectionReason: ''
        })
    }, [activeListId, sessionId])

    const updateMode = useCallback(async () => {
        const payload = buildModePayload({
            sessionId,
            listId: activeListId,
            mode,
            batchSize
        })

        await callApi('/api/modes', {
            method: 'PUT',
            headers: {'Content-Type': 'application/json'},
            body: JSON.stringify(payload)
        })

        onStatus('학습 모드를 적용했습니다.')
    }, [sessionId, activeListId, mode, batchSize, onStatus])

    useEffect(() => {
        cancelEditQuestion()
    }, [activeListId, cancelEditQuestion])

    return {
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
    }
}
