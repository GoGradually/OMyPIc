import {useCallback, useMemo, useState} from 'react'
import {callApi} from '../../../shared/api/http.js'

export function useRulebooks() {
    const [rulebooks, setRulebooks] = useState([])

    const refreshRulebooks = useCallback(async () => {
        const response = await callApi('/api/rulebooks')
        const data = await response.json()
        setRulebooks(data)
    }, [])

    const uploadRulebook = useCallback(async (file) => {
        const form = new FormData()
        form.append('file', file)
        await callApi('/api/rulebooks', {method: 'POST', body: form})
        await refreshRulebooks()
    }, [refreshRulebooks])

    const toggleRulebook = useCallback(async (id, enabled) => {
        await callApi(`/api/rulebooks/${id}/toggle?enabled=${enabled}`, {method: 'PUT'})
        await refreshRulebooks()
    }, [refreshRulebooks])

    const deleteRulebook = useCallback(async (id) => {
        await callApi(`/api/rulebooks/${id}`, {method: 'DELETE'})
        await refreshRulebooks()
    }, [refreshRulebooks])

    const enabledRulebookCount = useMemo(() => rulebooks.filter((book) => book.enabled).length, [rulebooks])

    return {
        rulebooks,
        enabledRulebookCount,
        refreshRulebooks,
        uploadRulebook,
        toggleRulebook,
        deleteRulebook
    }
}
