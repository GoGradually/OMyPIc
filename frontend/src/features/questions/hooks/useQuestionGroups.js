import {useCallback, useEffect, useMemo, useState} from 'react'
import {callApi} from '../../../shared/api/http.js'
import {buildModePayload} from '../../../shared/utils/mode.js'

function parseTagInput(rawInput) {
    if (!rawInput) {
        return []
    }
    return [...new Set(
        rawInput
            .split(',')
            .map((tag) => tag.trim().toLowerCase())
            .filter(Boolean)
    )]
}

export function useQuestionGroups({
                                      sessionId,
                                      onStatus
                                  }) {
    const [questionGroups, setQuestionGroups] = useState([])
    const [tagStats, setTagStats] = useState([])
    const [selectedGroupTags, setSelectedGroupTags] = useState([])
    const [activeGroupId, setActiveGroupId] = useState('')
    const [newGroupName, setNewGroupName] = useState('')
    const [newGroupTagsInput, setNewGroupTagsInput] = useState('')
    const [newQuestion, setNewQuestion] = useState('')
    const [newQuestionType, setNewQuestionType] = useState('')

    const [editingQuestionId, setEditingQuestionId] = useState('')
    const [editingQuestionText, setEditingQuestionText] = useState('')
    const [editingQuestionType, setEditingQuestionType] = useState('')

    const [mode, setMode] = useState('IMMEDIATE')
    const [batchSize, setBatchSize] = useState(3)
    const [currentQuestion, setCurrentQuestion] = useState(null)

    const activeQuestionGroup = useMemo(
        () => questionGroups.find((group) => group.id === activeGroupId) || null,
        [questionGroups, activeGroupId]
    )

    const cancelEditQuestion = useCallback(() => {
        setEditingQuestionId('')
        setEditingQuestionText('')
        setEditingQuestionType('')
    }, [])

    const refreshTagStats = useCallback(async () => {
        const response = await callApi('/api/question-groups/tags/stats')
        const data = await response.json()
        setTagStats(data)
        setSelectedGroupTags((prev) => prev.filter((tag) => data.some((item) => item.tag === tag && item.selectable)))
    }, [])

    const refreshQuestionGroups = useCallback(async () => {
        const response = await callApi('/api/question-groups')
        const data = await response.json()
        setQuestionGroups(data)
        setActiveGroupId((prev) => {
            if (!data.length) {
                return ''
            }
            if (prev && data.some((group) => group.id === prev)) {
                return prev
            }
            return data[0].id
        })
        await refreshTagStats()
    }, [refreshTagStats])

    const createGroup = useCallback(async () => {
        if (!newGroupName.trim()) {
            return
        }
        await callApi('/api/question-groups', {
            method: 'POST',
            headers: {'Content-Type': 'application/json'},
            body: JSON.stringify({
                name: newGroupName,
                tags: parseTagInput(newGroupTagsInput)
            })
        })
        setNewGroupName('')
        setNewGroupTagsInput('')
        await refreshQuestionGroups()
    }, [newGroupName, newGroupTagsInput, refreshQuestionGroups])

    const deleteGroup = useCallback(async () => {
        if (!activeGroupId) {
            return
        }
        await callApi(`/api/question-groups/${activeGroupId}`, {method: 'DELETE'})
        setCurrentQuestion(null)
        await refreshQuestionGroups()
    }, [activeGroupId, refreshQuestionGroups])

    const addQuestion = useCallback(async () => {
        if (!activeGroupId || !newQuestion.trim()) {
            return
        }
        await callApi(`/api/question-groups/${activeGroupId}/items`, {
            method: 'POST',
            headers: {'Content-Type': 'application/json'},
            body: JSON.stringify({text: newQuestion, questionType: newQuestionType || null})
        })
        setNewQuestion('')
        setNewQuestionType('')
        await refreshQuestionGroups()
    }, [activeGroupId, newQuestion, newQuestionType, refreshQuestionGroups])

    const startEditQuestion = useCallback((item) => {
        setEditingQuestionId(item.id)
        setEditingQuestionText(item.text || '')
        setEditingQuestionType(item.questionType || '')
    }, [])

    const saveEditedQuestion = useCallback(async () => {
        if (!activeGroupId || !editingQuestionId || !editingQuestionText.trim()) {
            return
        }
        await callApi(`/api/question-groups/${activeGroupId}/items/${editingQuestionId}`, {
            method: 'PUT',
            headers: {'Content-Type': 'application/json'},
            body: JSON.stringify({text: editingQuestionText, questionType: editingQuestionType || null})
        })
        cancelEditQuestion()
        await refreshQuestionGroups()
    }, [
        activeGroupId,
        editingQuestionId,
        editingQuestionText,
        editingQuestionType,
        cancelEditQuestion,
        refreshQuestionGroups
    ])

    const removeQuestion = useCallback(async (itemId) => {
        if (!activeGroupId || !itemId) {
            return
        }
        await callApi(`/api/question-groups/${activeGroupId}/items/${itemId}`, {method: 'DELETE'})
        if (editingQuestionId === itemId) {
            cancelEditQuestion()
        }
        await refreshQuestionGroups()
    }, [activeGroupId, editingQuestionId, cancelEditQuestion, refreshQuestionGroups])

    const toggleSelectedTag = useCallback((tag, selectable) => {
        if (!selectable) {
            return
        }
        setSelectedGroupTags((prev) => (
            prev.includes(tag)
                ? prev.filter((item) => item !== tag)
                : [...prev, tag]
        ))
    }, [])

    const nextQuestion = useCallback(async () => {
        const response = await callApi(`/api/question-groups/next?sessionId=${sessionId}`)
        const data = await response.json()
        setCurrentQuestion({
            ...data,
            exhausted: Boolean(data?.skipped),
            selectionReason: ''
        })
    }, [sessionId])

    const updateMode = useCallback(async () => {
        if (!selectedGroupTags.length) {
            throw new Error('질문 그룹 태그를 하나 이상 선택해 주세요.')
        }

        const payload = buildModePayload({
            sessionId,
            mode,
            batchSize,
            selectedGroupTags
        })

        await callApi('/api/modes', {
            method: 'PUT',
            headers: {'Content-Type': 'application/json'},
            body: JSON.stringify(payload)
        })

        onStatus('학습 모드를 적용했습니다.')
    }, [sessionId, mode, batchSize, selectedGroupTags, onStatus])

    useEffect(() => {
        cancelEditQuestion()
    }, [activeGroupId, cancelEditQuestion])

    return {
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
        nextQuestion,
        updateMode,
        setCurrentQuestion
    }
}
