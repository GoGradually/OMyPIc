import {useCallback, useEffect, useState} from 'react'
import {INITIAL_AUDIO_DEVICE_STATUS} from './voiceSessionConstants.js'

function getCurrentTimeLabel() {
    return new Date().toLocaleTimeString('ko-KR', {hour12: false})
}

export function useAudioDevices({
                                    sessionActiveRef,
                                    streamRef,
                                    setStatus
                                }) {
    const [audioPermission, setAudioPermission] = useState('unknown')
    const [audioDeviceStatus, setAudioDeviceStatus] = useState(INITIAL_AUDIO_DEVICE_STATUS)

    const refreshAudioDeviceStatus = useCallback(async ({requestPermission = false} = {}) => {
        if (!navigator.mediaDevices || !navigator.mediaDevices.enumerateDevices) {
            setAudioPermission('unsupported')
            setAudioDeviceStatus({
                ...INITIAL_AUDIO_DEVICE_STATUS,
                lastCheckedAt: getCurrentTimeLabel()
            })
            return
        }

        if (requestPermission && !sessionActiveRef.current) {
            try {
                const permissionStream = await navigator.mediaDevices.getUserMedia({audio: true})
                permissionStream.getTracks().forEach((track) => track.stop())
                setAudioPermission('granted')
                setStatus('오디오 장비 권한을 허용했습니다.')
            } catch (error) {
                if (error?.name === 'NotAllowedError' || error?.name === 'SecurityError') {
                    setAudioPermission('denied')
                    setStatus('마이크 권한이 거부되었습니다. 브라우저 설정에서 허용해 주세요.')
                } else if (error?.name === 'NotFoundError') {
                    setStatus('사용 가능한 마이크 장치를 찾지 못했습니다.')
                } else {
                    setStatus(`오디오 권한 요청 실패: ${error?.message || '알 수 없는 오류'}`)
                }
            }
        }

        try {
            const devices = await navigator.mediaDevices.enumerateDevices()
            const audioInputs = devices.filter((device) => device.kind === 'audioinput')
            const audioOutputs = devices.filter((device) => device.kind === 'audiooutput')
            const liveTrack = streamRef.current?.getAudioTracks?.()[0] || null
            const activeInputLabel = liveTrack?.label || audioInputs.find((device) => device.label)?.label || ''

            if (audioInputs.some((device) => Boolean(device.label))) {
                setAudioPermission((prev) => (prev === 'denied' ? prev : 'granted'))
            }

            setAudioDeviceStatus({
                inputCount: audioInputs.length,
                outputCount: audioOutputs.length,
                activeInputLabel,
                liveInput: Boolean(liveTrack && liveTrack.readyState === 'live'),
                lastCheckedAt: getCurrentTimeLabel()
            })
        } catch (error) {
            setStatus(`오디오 장치 상태 확인 실패: ${error?.message || '알 수 없는 오류'}`)
        }
    }, [sessionActiveRef, setStatus, streamRef])

    const handleAudioQuickAction = useCallback(async () => {
        if (audioPermission === 'unsupported') {
            return
        }
        if (audioPermission === 'granted') {
            await refreshAudioDeviceStatus()
            setStatus('오디오 연결 상태를 확인했습니다.')
            return
        }
        await refreshAudioDeviceStatus({requestPermission: true})
    }, [audioPermission, refreshAudioDeviceStatus, setStatus])

    useEffect(() => {
        refreshAudioDeviceStatus().catch(() => {
        })
    }, [refreshAudioDeviceStatus])

    useEffect(() => {
        if (!navigator.mediaDevices || !navigator.mediaDevices.addEventListener) {
            return
        }

        const handleDeviceChange = () => {
            refreshAudioDeviceStatus().catch(() => {
            })
        }

        navigator.mediaDevices.addEventListener('devicechange', handleDeviceChange)
        return () => {
            navigator.mediaDevices.removeEventListener('devicechange', handleDeviceChange)
        }
    }, [refreshAudioDeviceStatus])

    useEffect(() => {
        if (!navigator.permissions || !navigator.permissions.query) {
            return
        }

        let cancelled = false
        let permissionStatus = null

        async function observeMicrophonePermission() {
            try {
                permissionStatus = await navigator.permissions.query({name: 'microphone'})
                if (cancelled || !permissionStatus) {
                    return
                }
                setAudioPermission(permissionStatus.state)
                permissionStatus.onchange = () => {
                    setAudioPermission(permissionStatus.state)
                    refreshAudioDeviceStatus().catch(() => {
                    })
                }
            } catch (_error) {
            }
        }

        observeMicrophonePermission().catch(() => {
        })

        return () => {
            cancelled = true
            if (permissionStatus) {
                permissionStatus.onchange = null
            }
        }
    }, [refreshAudioDeviceStatus])

    return {
        audioPermission,
        setAudioPermission,
        audioDeviceStatus,
        refreshAudioDeviceStatus,
        handleAudioQuickAction
    }
}
