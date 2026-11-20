import {useCallback, useEffect, useMemo, useState} from 'react'
import {callApi} from '../../../shared/api/http.js'

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

// Runs one change (create, edit, delete). A refusal from the backend (for example a fourth question
// in a group) or a network failure is told to onError instead of being lost as an unhandled
// rejection. Without onError the failure is thrown as before.
function reportingFailures(action, label, onError) {
    return async (...args) => {
        try {
            return await action(...args)
        } catch (error) {
            if (!onError) {
                throw error
            }
            onError(`${label}에 실패했습니다: ${error?.message || '알 수 없는 오류'}`)
            return undefined
        }
    }
}

export function useQuestionManager({onGroupsRefreshed, onError} = {}) {
    const [questionGroups, setQuestionGroups] = useState([])
    const [activeGroupId, setActiveGroupId] = useState('')
    const [isCreateGroupModalOpen, setIsCreateGroupModalOpen] = useState(false)
    const [newGroupName, setNewGroupName] = useState('')
    const [newGroupTagsInput, setNewGroupTagsInput] = useState('')
    const [isEditGroupModalOpen, setIsEditGroupModalOpen] = useState(false)
    const [editingGroupId, setEditingGroupId] = useState('')
    const [editingGroupName, setEditingGroupName] = useState('')
    const [editingGroupTagsInput, setEditingGroupTagsInput] = useState('')
    const [newQuestion, setNewQuestion] = useState('')
    const [newQuestionType, setNewQuestionType] = useState('')

    const [editingQuestionId, setEditingQuestionId] = useState('')
    const [editingQuestionText, setEditingQuestionText] = useState('')
    const [editingQuestionType, setEditingQuestionType] = useState('')

    const activeQuestionGroup = useMemo(
        () => questionGroups.find((group) => group.id === activeGroupId) || null,
        [questionGroups, activeGroupId]
    )

    const cancelEditQuestion = useCallback(() => {
        setEditingQuestionId('')
        setEditingQuestionText('')
        setEditingQuestionType('')
    }, [])

    const openCreateGroupModal = useCallback(() => {
        setNewGroupName('')
        setNewGroupTagsInput('')
        setIsCreateGroupModalOpen(true)
    }, [])

    const closeCreateGroupModal = useCallback(() => {
        setIsCreateGroupModalOpen(false)
        setNewGroupName('')
        setNewGroupTagsInput('')
    }, [])

    const openEditGroupModal = useCallback((group) => {
        if (!group || !group.id) {
            return
        }
        setActiveGroupId(group.id)
        setEditingGroupId(group.id)
        setEditingGroupName(group.name || '')
        setEditingGroupTagsInput((group.tags || []).join(', '))
        setIsEditGroupModalOpen(true)
    }, [])

    const closeEditGroupModal = useCallback(() => {
        setIsEditGroupModalOpen(false)
        setEditingGroupId('')
        setEditingGroupName('')
        setEditingGroupTagsInput('')
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
        await onGroupsRefreshed?.()
    }, [onGroupsRefreshed])

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
        setIsCreateGroupModalOpen(false)
        await refreshQuestionGroups()
    }, [newGroupName, newGroupTagsInput, refreshQuestionGroups])

    const deleteGroup = useCallback(async (groupId) => {
        const targetGroupId = groupId || activeGroupId
        if (!targetGroupId) {
            return
        }
        await callApi(`/api/question-groups/${targetGroupId}`, {method: 'DELETE'})
        if (targetGroupId === editingGroupId) {
            closeEditGroupModal()
        }
        await refreshQuestionGroups()
    }, [activeGroupId, editingGroupId, closeEditGroupModal, refreshQuestionGroups])

    const saveEditedGroup = useCallback(async () => {
        if (!editingGroupId || !editingGroupName.trim()) {
            return
        }
        await callApi(`/api/question-groups/${editingGroupId}`, {
            method: 'PUT',
            headers: {'Content-Type': 'application/json'},
            body: JSON.stringify({
                name: editingGroupName,
                tags: parseTagInput(editingGroupTagsInput)
            })
        })
        setIsEditGroupModalOpen(false)
        setEditingGroupId('')
        await refreshQuestionGroups()
    }, [
        editingGroupId,
        editingGroupName,
        editingGroupTagsInput,
        refreshQuestionGroups
    ])

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

    useEffect(() => {
        cancelEditQuestion()
    }, [activeGroupId, cancelEditQuestion])

    // What the screens call: a failure reaches the status bar.
    const guarded = useMemo(() => ({
        createGroup: reportingFailures(createGroup, '질문 그룹 추가', onError),
        deleteGroup: reportingFailures(deleteGroup, '질문 그룹 삭제', onError),
        saveEditedGroup: reportingFailures(saveEditedGroup, '질문 그룹 수정', onError),
        addQuestion: reportingFailures(addQuestion, '질문 추가', onError),
        saveEditedQuestion: reportingFailures(saveEditedQuestion, '질문 수정', onError),
        removeQuestion: reportingFailures(removeQuestion, '질문 삭제', onError)
    }), [createGroup, deleteGroup, saveEditedGroup, addQuestion, saveEditedQuestion, removeQuestion, onError])

    return {
        questionGroups,
        activeGroupId,
        setActiveGroupId,
        activeQuestionGroup,
        isCreateGroupModalOpen,
        newGroupName,
        setNewGroupName,
        newGroupTagsInput,
        setNewGroupTagsInput,
        isEditGroupModalOpen,
        editingGroupName,
        setEditingGroupName,
        editingGroupTagsInput,
        setEditingGroupTagsInput,
        newQuestion,
        setNewQuestion,
        newQuestionType,
        setNewQuestionType,
        editingQuestionId,
        editingQuestionText,
        setEditingQuestionText,
        editingQuestionType,
        setEditingQuestionType,
        refreshQuestionGroups,
        openCreateGroupModal,
        closeCreateGroupModal,
        createGroup: guarded.createGroup,
        openEditGroupModal,
        closeEditGroupModal,
        deleteGroup: guarded.deleteGroup,
        saveEditedGroup: guarded.saveEditedGroup,
        addQuestion: guarded.addQuestion,
        startEditQuestion,
        cancelEditQuestion,
        saveEditedQuestion: guarded.saveEditedQuestion,
        removeQuestion: guarded.removeQuestion
    }
}
