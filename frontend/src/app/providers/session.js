import {useMemo} from 'react'

export function useSessionId(storageKey = 'omypic_session') {
    return useMemo(() => {
        const stored = localStorage.getItem(storageKey)
        if (stored) {
            return stored
        }
        const id = crypto.randomUUID()
        localStorage.setItem(storageKey, id)
        return id
    }, [storageKey])
}
