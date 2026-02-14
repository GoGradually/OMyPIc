export function parseSseData(raw) {
    if (!raw) {
        return {}
    }
    try {
        return JSON.parse(raw)
    } catch (_error) {
        return {}
    }
}

export function joinByteChunks(chunks, totalSize) {
    const merged = new Uint8Array(totalSize)
    let offset = 0
    for (const chunk of chunks) {
        merged.set(chunk, offset)
        offset += chunk.length
    }
    return merged
}

export function computeRms(float32Array) {
    if (!float32Array || !float32Array.length) {
        return 0
    }
    let sum = 0
    for (let i = 0; i < float32Array.length; i += 1) {
        const sample = float32Array[i]
        sum += sample * sample
    }
    return Math.sqrt(sum / float32Array.length)
}
