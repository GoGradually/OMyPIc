// Turns the library's per-file download events into one percentage for the whole model.
//
// The library reports each file on its own, and the small files (config.json, 44 bytes) finish
// before the large one starts. Showing a file's own percentage would say 100% while almost
// nothing has been downloaded, so the bytes of all files are added up against the size of the
// whole download. expectedBytes is that size when it is known (the pinned default models); when
// it is not, the sizes of the files seen so far stand in for it.
export function createDownloadProgress(expectedBytes = 0) {
    const files = new Map()
    let highest = 0
    return ({file = '', loaded, total} = {}) => {
        if (!Number.isFinite(loaded) || !Number.isFinite(total) || total <= 0) {
            return null
        }
        files.set(file, {loaded: Math.min(Math.max(loaded, 0), total), total})
        let loadedBytes = 0
        let totalBytes = 0
        for (const entry of files.values()) {
            loadedBytes += entry.loaded
            totalBytes += entry.total
        }
        const percent = Math.min(100, (loadedBytes / Math.max(totalBytes, expectedBytes)) * 100)
        // A file larger than expected must not make the number go backwards.
        highest = Math.max(highest, percent)
        return highest
    }
}
