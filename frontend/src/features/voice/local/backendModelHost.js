// kokoro-js downloads its voice files (voices/*.bin) from a hard-coded Hugging Face address
// (kokoro-js 1.2.1), bypassing env.remoteHost. When the backend serves the model files, this
// sends those requests to the backend as well. Only used inside the speech synthesis worker.

const HUGGING_FACE = 'https://huggingface.co/'

export function redirectHuggingFaceFetch(scope, modelHost) {
    const originalFetch = scope.fetch.bind(scope)
    scope.fetch = (input, init) => {
        const url = typeof input === 'string' ? input : input instanceof URL ? input.href : input?.url
        if (typeof url !== 'string' || !url.startsWith(HUGGING_FACE)) {
            return originalFetch(input, init)
        }
        const target = modelHost + url.slice(HUGGING_FACE.length)
        return originalFetch(typeof input === 'string' || input instanceof URL ? target : new Request(target, input), init)
    }
}
