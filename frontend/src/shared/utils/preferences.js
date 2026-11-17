const PREFIX = 'omypic_pref_'

// Learner preferences that should survive an app restart. Storage can be unavailable
// (private mode, blocked site data), so every access falls back to the default.
export function readPreference(key, fallback) {
    try {
        const raw = window.localStorage.getItem(PREFIX + key)
        return raw === null ? fallback : JSON.parse(raw)
    } catch (_error) {
        return fallback
    }
}

export function writePreference(key, value) {
    try {
        window.localStorage.setItem(PREFIX + key, JSON.stringify(value))
    } catch (_error) {
        // The preference simply resets on next launch.
    }
}
