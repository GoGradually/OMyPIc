// The text shown when the server reports that it stopped the session (recovery after a reconnect).
export function stoppedByServerMessage(reason) {
    if (!reason) {
        return '세션이 서버에서 종료되었습니다.'
    }
    if (reason === 'idle_timeout') {
        // The backend stops a session whose event stream stayed disconnected for the idle period.
        return '오랫동안 연결이 끊겨 있어 세션이 종료되었습니다. 다시 시작해 주세요.'
    }
    return `세션이 종료되었습니다. (${reason})`
}
