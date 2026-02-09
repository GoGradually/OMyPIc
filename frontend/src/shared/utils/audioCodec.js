function encodeBinary(binary) {
    if (typeof btoa === 'function') {
        return btoa(binary)
    }
    return Buffer.from(binary, 'binary').toString('base64')
}

function decodeBase64(base64) {
    if (typeof atob === 'function') {
        return atob(base64)
    }
    return Buffer.from(base64, 'base64').toString('binary')
}

export function float32ToInt16(float32Array) {
    const output = new Int16Array(float32Array.length)
    for (let i = 0; i < float32Array.length; i += 1) {
        const sample = Math.max(-1, Math.min(1, float32Array[i]))
        output[i] = sample < 0 ? sample * 0x8000 : sample * 0x7fff
    }
    return output
}

export function toBase64(bytes) {
    let binary = ''
    const chunkSize = 0x8000

    for (let i = 0; i < bytes.length; i += chunkSize) {
        const chunk = bytes.subarray(i, i + chunkSize)
        binary += String.fromCharCode(...chunk)
    }

    return encodeBinary(binary)
}

export function fromBase64(base64) {
    const binary = decodeBase64(base64)
    const bytes = new Uint8Array(binary.length)

    for (let i = 0; i < binary.length; i += 1) {
        bytes[i] = binary.charCodeAt(i)
    }

    return bytes
}

export function mergeAudioChunks(base64Chunks) {
    const chunks = base64Chunks.map(fromBase64)
    const totalSize = chunks.reduce((sum, chunk) => sum + chunk.length, 0)
    const merged = new Uint8Array(totalSize)

    let offset = 0
    for (const chunk of chunks) {
        merged.set(chunk, offset)
        offset += chunk.length
    }

    return merged
}
