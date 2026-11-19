/**
 * 확인 창을 띄우고 사용자가 승인했을 때만 `true`로 끝난다.
 *
 * 브라우저의 `window.confirm`은 boolean을 바로 돌려주지만, Tauri 앱에서는 dialog 플러그인이
 * `window.confirm`을 네이티브 대화상자를 여는 async 함수로 바꿔 둔다(Promise를 돌려준다).
 * 그래서 결과를 항상 기다린다. 확인 창을 띄울 수 없거나 실패하면 거절로 본다.
 *
 * @param {string} message
 * @param {{confirm?: (message: string) => boolean | Promise<boolean>}} [target]
 * @returns {Promise<boolean>}
 */
export async function confirmAction(message, target = window) {
    if (!target || typeof target.confirm !== 'function') {
        return false
    }
    try {
        return (await target.confirm(message)) === true
    } catch (error) {
        console.error('Cannot show the confirmation dialog:', error)
        return false
    }
}
