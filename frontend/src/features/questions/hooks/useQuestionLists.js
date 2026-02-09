import {useCallback, useEffect, useMemo, useState} from 'react'
import {callApi} from '../../../shared/api/http.js'
import {buildModePayload} from '../../../shared/utils/mode.js'

export function useQuestionLists({
                                     sessionId,
                                     provider,
                                     mockFinalModel,
                                     feedbackLang,
                                     onFeedback,
                                     onStatus,
                                     refreshWrongNotes
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
    const [mockOrder, setMockOrder] = useState('')
    const [mockCounts, setMockCounts] = useState('{}')
    const [mockFinalRequested, setMockFinalRequested] = useState(false)

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
        setMockFinalRequested(false)
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

    const requestMockFinalFeedback = useCallback(async () => {
        const response = await callApi('/api/feedback/mock-final', {
            method: 'POST',
            headers: {'Content-Type': 'application/json'},
            body: JSON.stringify({
                sessionId,
                provider,
                model: mockFinalModel,
                feedbackLanguage: feedbackLang
            })
        }, provider)
        const envelope = await response.json()
        if (envelope.generated && envelope.feedback) {
            onFeedback(envelope.feedback)
            await refreshWrongNotes()
        }
    }, [sessionId, provider, mockFinalModel, feedbackLang, onFeedback, refreshWrongNotes])

    const nextQuestion = useCallback(async () => {
        if (!activeListId) {
            return
        }
        const response = await callApi(`/api/questions/${activeListId}/next?sessionId=${sessionId}`)
        const data = await response.json()
        setCurrentQuestion(data)

        if (data.mockExamCompleted && !mockFinalRequested) {
            setMockFinalRequested(true)
            try {
                await requestMockFinalFeedback()
                onStatus('모의고사 최종 피드백을 생성했습니다.')
            } catch (error) {
                setMockFinalRequested(false)
                onStatus(`모의고사 최종 피드백 실패: ${error.message}`)
            }
        }
    }, [
        activeListId,
        sessionId,
        mockFinalRequested,
        requestMockFinalFeedback,
        onStatus
    ])

    const updateMode = useCallback(async () => {
        const payload = buildModePayload({
            sessionId,
            listId: activeListId,
            mode,
            batchSize,
            mockOrder,
            mockCounts
        })

        await callApi('/api/modes', {
            method: 'PUT',
            headers: {'Content-Type': 'application/json'},
            body: JSON.stringify(payload)
        })

        setMockFinalRequested(false)
        onStatus('학습 모드를 적용했습니다.')
    }, [sessionId, activeListId, mode, batchSize, mockOrder, mockCounts, onStatus])

    useEffect(() => {
        if (mode !== 'MOCK_EXAM') {
            setMockFinalRequested(false)
        }
    }, [mode])

    useEffect(() => {
        setMockFinalRequested(false)
    }, [activeListId])

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
    }
}
