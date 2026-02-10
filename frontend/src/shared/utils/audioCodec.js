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

function writeAscii(view, offset, text) {
    for (let i = 0; i < text.length; i += 1) {
        view.setUint8(offset + i, text.charCodeAt(i))
    }
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

export function pcm16ToWav(pcmBytes, sampleRate = 24000, channelCount = 1) {
    const bytesPerSample = 2
    const blockAlign = channelCount * bytesPerSample
    const byteRate = sampleRate * blockAlign
    const dataSize = pcmBytes.length
    const wavBuffer = new ArrayBuffer(44 + dataSize)
    const view = new DataView(wavBuffer)

    writeAscii(view, 0, 'RIFF')
    view.setUint32(4, 36 + dataSize, true)
    writeAscii(view, 8, 'WAVE')
    writeAscii(view, 12, 'fmt ')
    view.setUint32(16, 16, true)
    view.setUint16(20, 1, true)
    view.setUint16(22, channelCount, true)
    view.setUint32(24, sampleRate, true)
    view.setUint32(28, byteRate, true)
    view.setUint16(32, blockAlign, true)
    view.setUint16(34, bytesPerSample * 8, true)
    writeAscii(view, 36, 'data')
    view.setUint32(40, dataSize, true)

    const wavBytes = new Uint8Array(wavBuffer)
    wavBytes.set(pcmBytes, 44)
    return wavBytes
}
