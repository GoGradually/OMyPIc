import {useCallback, useState} from 'react'
import {callApi} from '../../../shared/api/http.js'
import {buildModePayload} from '../../../shared/utils/mode.js'

export function useLearningMode({
                                    sessionId,
                                    onStatus
                                }) {
    const [tagStats, setTagStats] = useState([])
    const [selectedGroupTags, setSelectedGroupTags] = useState([])
    const [mode, setMode] = useState('IMMEDIATE')
    const [batchSize, setBatchSize] = useState(3)

    const refreshTagStats = useCallback(async () => {
        const response = await callApi('/api/question-groups/tags/stats')
        const data = await response.json()
        setTagStats(data)
        setSelectedGroupTags((prev) => prev.filter((tag) => data.some((item) => item.tag === tag && item.selectable)))
    }, [])

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

        onStatus?.('학습 모드를 적용했습니다.')
    }, [sessionId, mode, batchSize, selectedGroupTags, onStatus])

    return {
        tagStats,
        selectedGroupTags,
        mode,
        setMode,
        batchSize,
        setBatchSize,
        refreshTagStats,
        toggleSelectedTag,
        updateMode
    }
}
