export async function copyText(value) {
    if (navigator.clipboard && navigator.clipboard.writeText) {
        await navigator.clipboard.writeText(value)
        return
    }

    const helper = document.createElement('textarea')
    helper.value = value
    helper.setAttribute('readonly', '')
    helper.style.position = 'absolute'
    helper.style.left = '-9999px'

    document.body.append(helper)
    helper.select()
    document.execCommand('copy')
    document.body.removeChild(helper)
}
