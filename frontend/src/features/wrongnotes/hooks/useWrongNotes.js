import {useCallback, useState} from 'react'
import {callApi} from '../../../shared/api/http.js'

export function useWrongNotes() {
    const [wrongNotes, setWrongNotes] = useState([])

    const refreshWrongNotes = useCallback(async () => {
        const response = await callApi('/api/wrongnotes')
        const data = await response.json()
        setWrongNotes(data)
    }, [])

    return {
        wrongNotes,
        refreshWrongNotes
    }
}
