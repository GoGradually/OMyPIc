import React, {useEffect, useMemo, useRef, useState} from 'react'
import {
  callApi,
  closeRealtime,
  connectRealtime,
  getApiKey,
  sendRealtime,
  setApiKey,
  subscribeRealtime,
  verifyApiKey
} from './api.js'

const defaultModels = {
    openai: ['gpt-4o-mini', 'gpt-4.1-mini', 'gpt-4.1'],
    anthropic: ['claude-3-5-sonnet-20240620', 'claude-3-haiku-20240307'],
    gemini: ['gemini-1.5-pro', 'gemini-1.5-flash']
}

const voices = ['alloy', 'echo', 'fable', 'nova', 'shimmer']

function float32ToInt16(float32Array) {
    const output = new Int16Array(float32Array.length)
    for (let i = 0; i < float32Array.length; i += 1) {
        const s = Math.max(-1, Math.min(1, float32Array[i]))
        output[i] = s < 0 ? s * 0x8000 : s * 0x7fff
    }
    return output
}

function toBase64(bytes) {
    let binary = ''
    const chunkSize = 0x8000
    for (let i = 0; i < bytes.length; i += chunkSize) {
        const chunk = bytes.subarray(i, i + chunkSize)
        binary += String.fromCharCode(...chunk)
    }
    return btoa(binary)
}

function fromBase64(base64) {
    const binary = atob(base64)
    const bytes = new Uint8Array(binary.length)
    for (let i = 0; i < binary.length; i += 1) {
        bytes[i] = binary.charCodeAt(i)
    }
    return bytes
}

export default function App() {
    const sessionId = useMemo(() => {
        const stored = localStorage.getItem('omypic_session')
        if (stored) return stored
        const id = crypto.randomUUID()
        localStorage.setItem('omypic_session', id)
        return id
    }, [])

    const [provider, setProvider] = useState('openai')
    const [model, setModel] = useState(defaultModels.openai[0])
    const [apiKeyInput, setApiKeyInput] = useState('')
    const [apiKeyStatus, setApiKeyStatus] = useState('')
    const [feedbackLang, setFeedbackLang] = useState('ko')
    const [mode, setMode] = useState('IMMEDIATE')
    const [batchSize, setBatchSize] = useState(3)
    const [mockOrder, setMockOrder] = useState('')
    const [mockCounts, setMockCounts] = useState('{}')
    const [recording, setRecording] = useState(false)
    const [realtimeConnected, setRealtimeConnected] = useState(false)
    const [partialTranscript, setPartialTranscript] = useState('')
    const [transcript, setTranscript] = useState('')
    const [userText, setUserText] = useState('')
    const [feedback, setFeedback] = useState(null)
    const [rulebooks, setRulebooks] = useState([])
    const [questionLists, setQuestionLists] = useState([])
    const [activeListId, setActiveListId] = useState('')
    const [newListName, setNewListName] = useState('')
    const [newQuestion, setNewQuestion] = useState('')
    const [newGroup, setNewGroup] = useState('')
    const [editingQuestionId, setEditingQuestionId] = useState('')
    const [editingQuestionText, setEditingQuestionText] = useState('')
    const [editingQuestionGroup, setEditingQuestionGroup] = useState('')
    const [currentQuestion, setCurrentQuestion] = useState(null)
    const [mockFinalRequested, setMockFinalRequested] = useState(false)
    const [wrongNotes, setWrongNotes] = useState([])
    const [voice, setVoice] = useState(voices[0])

    const realtimeSocketIdRef = useRef(null)
    const ttsChunksRef = useRef(new Map())
    const recordingRef = useRef(false)
    const streamRef = useRef(null)
    const audioContextRef = useRef(null)
    const sourceNodeRef = useRef(null)
    const processorRef = useRef(null)
    const activeQuestionList = questionLists.find(list => list.id === activeListId) || null

    useEffect(() => {
        refreshRulebooks()
        refreshQuestionLists()
        refreshWrongNotes()
    }, [])

    useEffect(() => {
        setModel(defaultModels[provider][0])
        getApiKey(provider).then(key => setApiKeyInput(key || ''))
        setApiKeyStatus('')
    }, [provider])

    useEffect(() => {
        const unsubscribe = subscribeRealtime((event) => {
            if (!event || event.socketId !== realtimeSocketIdRef.current) {
                return
            }
            if (event.type === 'open') {
                setRealtimeConnected(true)
                syncRealtimeSettings().catch(() => {
                })
                return
            }
            if (event.type === 'close') {
                setRealtimeConnected(false)
                realtimeSocketIdRef.current = null
                return
            }
            if (event.type === 'error') {
                setApiKeyStatus(`실시간 오류: ${event.data || 'unknown error'}`)
                return
            }
            if (event.type !== 'message') {
                return
            }

            try {
                const envelope = JSON.parse(event.data)
                const type = envelope?.type
                const data = envelope?.data || {}

                if (type === 'stt.partial') {
                    setPartialTranscript(data.text || '')
                    return
                }
                if (type === 'stt.final') {
                    setPartialTranscript('')
                    setTranscript(data.text || '')
                    setUserText(data.text || '')
                    return
                }
                if (type === 'feedback.final') {
                    setFeedback({
                        summary: data.summary || '',
                        correctionPoints: data.correctionPoints || [],
                        exampleAnswer: data.exampleAnswer || '',
                        rulebookEvidence: data.rulebookEvidence || []
                    })
                    refreshWrongNotes().catch(() => {
                    })
                    return
                }
                if (type === 'tts.chunk') {
                    const turnId = data.turnId
                    const existing = ttsChunksRef.current.get(turnId) || []
                    existing.push(data.audio)
                    ttsChunksRef.current.set(turnId, existing)
                    return
                }
                if (type === 'turn.completed') {
                    playTurnAudio(data.turnId)
                    return
                }
                if (type === 'error') {
                    const msg = typeof data === 'string' ? data : data.message
                    setApiKeyStatus(`오류: ${msg || 'unknown error'}`)
                }
            } catch (e) {
                setApiKeyStatus('실시간 이벤트 파싱 실패')
            }
        })

        return () => {
            if (unsubscribe) {
                unsubscribe()
            }
        }
    }, [])

    useEffect(() => {
        return () => {
            stopRecording()
            const socketId = realtimeSocketIdRef.current
            if (socketId) {
                closeRealtime(socketId).catch(() => {
                })
            }
        }
    }, [])

    useEffect(() => {
        if (!realtimeConnected) {
            return
        }
        syncRealtimeSettings().catch(() => {
        })
    }, [provider, model, feedbackLang, voice, realtimeConnected])

    useEffect(() => {
        recordingRef.current = recording
    }, [recording])

    useEffect(() => {
        if (mode !== 'MOCK_EXAM') {
            setMockFinalRequested(false)
        }
    }, [mode])

    useEffect(() => {
        setMockFinalRequested(false)
    }, [activeListId])

    useEffect(() => {
        cancelEditQuestion()
    }, [activeListId])

    async function refreshRulebooks() {
        const response = await callApi('/api/rulebooks')
        const data = await response.json()
        setRulebooks(data)
    }

    async function refreshQuestionLists() {
        const response = await callApi('/api/questions')
        const data = await response.json()
        setQuestionLists(data)
        setActiveListId(prev => {
            if (!data.length) {
                return ''
            }
            if (prev && data.some(list => list.id === prev)) {
                return prev
            }
            return data[0].id
        })
    }

    async function refreshWrongNotes() {
        const response = await callApi('/api/wrongnotes')
        const data = await response.json()
        setWrongNotes(data)
    }

    async function handleSaveApiKey() {
        await setApiKey(provider, apiKeyInput)
        try {
            const result = await verifyApiKey(provider, apiKeyInput, model)
            if (result.valid) {
                setApiKeyStatus('API Key 검증 성공')
            } else {
                setApiKeyStatus(`API Key 검증 실패: ${result.message}`)
            }
        } catch (error) {
            setApiKeyStatus(`API Key 검증 실패: ${error.message}`)
        }
    }

    async function ensureRealtimeConnected() {
        if (realtimeSocketIdRef.current) {
            return realtimeSocketIdRef.current
        }

        const openAiKey = await getApiKey('openai')
        if (!openAiKey) {
            throw new Error('실시간 음성을 위해 OpenAI API Key가 필요합니다.')
        }

        const socketId = await connectRealtime(sessionId, openAiKey)
        realtimeSocketIdRef.current = socketId
        return socketId
    }

    async function syncRealtimeSettings() {
        const socketId = realtimeSocketIdRef.current
        if (!socketId) {
            return
        }
        const feedbackApiKey = await getApiKey(provider)
        await sendRealtime(socketId, {
            type: 'session.update',
            data: {
                feedbackProvider: provider,
                feedbackModel: model,
                feedbackApiKey: feedbackApiKey || '',
                feedbackLanguage: feedbackLang,
                ttsVoice: voice
            }
        })
    }

    async function startRecording() {
        if (recordingRef.current) {
            return
        }

        try {
            const socketId = await ensureRealtimeConnected()
            await syncRealtimeSettings()

            const stream = await navigator.mediaDevices.getUserMedia({audio: true})
            const AudioContextClass = window.AudioContext || window.webkitAudioContext
            const audioContext = new AudioContextClass({sampleRate: 16000})
            const sourceNode = audioContext.createMediaStreamSource(stream)
            const processor = audioContext.createScriptProcessor(4096, 1, 1)

            processor.onaudioprocess = (event) => {
                if (!recordingRef.current) {
                    return
                }
                const channel = event.inputBuffer.getChannelData(0)
                const pcm = float32ToInt16(channel)
                const bytes = new Uint8Array(pcm.buffer)
                const base64Audio = toBase64(bytes)
                sendRealtime(socketId, {
                    type: 'audio.append',
                    data: {audio: base64Audio}
                }).catch(() => {
                })
            }

            sourceNode.connect(processor)
            processor.connect(audioContext.destination)

            streamRef.current = stream
            audioContextRef.current = audioContext
            sourceNodeRef.current = sourceNode
            processorRef.current = processor

            setRecording(true)
            recordingRef.current = true
            setApiKeyStatus('녹음 중...')
            setTimeout(() => stopRecording(), 180000)
        } catch (error) {
            setApiKeyStatus(error.message)
        }
    }

    function stopRecording() {
        if (!recordingRef.current && !processorRef.current && !streamRef.current) {
            return
        }

        setRecording(false)
        recordingRef.current = false

        if (processorRef.current) {
            processorRef.current.disconnect()
            processorRef.current.onaudioprocess = null
            processorRef.current = null
        }
        if (sourceNodeRef.current) {
            sourceNodeRef.current.disconnect()
            sourceNodeRef.current = null
        }
        if (audioContextRef.current) {
            audioContextRef.current.close().catch(() => {
            })
            audioContextRef.current = null
        }
        if (streamRef.current) {
            streamRef.current.getTracks().forEach(track => track.stop())
            streamRef.current = null
        }

        const socketId = realtimeSocketIdRef.current
        if (socketId) {
            sendRealtime(socketId, {type: 'audio.commit'}).catch(() => {
            })
        }
        setApiKeyStatus('녹음 완료, 피드백 대기 중...')
    }

    function playTurnAudio(turnId) {
        const chunks = ttsChunksRef.current.get(turnId)
        if (!chunks || !chunks.length) {
            return
        }
        ttsChunksRef.current.delete(turnId)

        const bytes = chunks.map(fromBase64)
        const total = bytes.reduce((sum, chunk) => sum + chunk.length, 0)
        const merged = new Uint8Array(total)
        let offset = 0
        for (const chunk of bytes) {
            merged.set(chunk, offset)
            offset += chunk.length
        }

        const audioBlob = new Blob([merged], {type: 'audio/mpeg'})
        const url = URL.createObjectURL(audioBlob)
        const audio = new Audio(url)
        audio.play().catch(() => {
        })
        audio.onended = () => URL.revokeObjectURL(url)
    }

    async function uploadRulebook(file) {
        const form = new FormData()
        form.append('file', file)
        await callApi('/api/rulebooks', {method: 'POST', body: form})
        refreshRulebooks()
    }

    async function toggleRulebook(id, enabled) {
        await callApi(`/api/rulebooks/${id}/toggle?enabled=${enabled}`, {method: 'PUT'})
        refreshRulebooks()
    }

    async function deleteRulebook(id) {
        await callApi(`/api/rulebooks/${id}`, {method: 'DELETE'})
        refreshRulebooks()
    }

    async function createList() {
        if (!newListName.trim()) return
        await callApi('/api/questions', {
            method: 'POST',
            headers: {'Content-Type': 'application/json'},
            body: JSON.stringify({name: newListName})
        })
        setNewListName('')
        refreshQuestionLists()
    }

    async function deleteList() {
        if (!activeListId) return
        await callApi(`/api/questions/${activeListId}`, {method: 'DELETE'})
        setCurrentQuestion(null)
        setMockFinalRequested(false)
        refreshQuestionLists()
    }

    async function addQuestion() {
        if (!activeListId || !newQuestion.trim()) return
        await callApi(`/api/questions/${activeListId}/items`, {
            method: 'POST',
            headers: {'Content-Type': 'application/json'},
            body: JSON.stringify({text: newQuestion, group: newGroup || null})
        })
        setNewQuestion('')
        setNewGroup('')
        refreshQuestionLists()
    }

    function startEditQuestion(item) {
        setEditingQuestionId(item.id)
        setEditingQuestionText(item.text || '')
        setEditingQuestionGroup(item.group || '')
    }

    function cancelEditQuestion() {
        setEditingQuestionId('')
        setEditingQuestionText('')
        setEditingQuestionGroup('')
    }

    async function saveEditedQuestion() {
        if (!activeListId || !editingQuestionId || !editingQuestionText.trim()) return
        await callApi(`/api/questions/${activeListId}/items/${editingQuestionId}`, {
            method: 'PUT',
            headers: {'Content-Type': 'application/json'},
            body: JSON.stringify({text: editingQuestionText, group: editingQuestionGroup || null})
        })
        cancelEditQuestion()
        refreshQuestionLists()
    }

    async function removeQuestion(itemId) {
        if (!activeListId || !itemId) return
        await callApi(`/api/questions/${activeListId}/items/${itemId}`, {method: 'DELETE'})
        if (editingQuestionId === itemId) {
            cancelEditQuestion()
        }
        refreshQuestionLists()
    }

    async function requestMockFinalFeedback() {
        const response = await callApi('/api/feedback/mock-final', {
            method: 'POST',
            headers: {'Content-Type': 'application/json'},
            body: JSON.stringify({
                sessionId,
                provider,
                model,
                feedbackLanguage: feedbackLang
            })
        }, provider)
        const envelope = await response.json()
        if (envelope.generated && envelope.feedback) {
            setFeedback(envelope.feedback)
            await refreshWrongNotes()
        }
    }

    async function nextQuestion() {
        if (!activeListId) return
        const response = await callApi(`/api/questions/${activeListId}/next?sessionId=${sessionId}`)
        const data = await response.json()
        setCurrentQuestion(data)
        if (data.mockExamCompleted && !mockFinalRequested) {
            setMockFinalRequested(true)
            try {
                await requestMockFinalFeedback()
                setApiKeyStatus('모의고사 최종 피드백 생성 완료')
            } catch (error) {
                setMockFinalRequested(false)
                setApiKeyStatus(`모의고사 최종 피드백 실패: ${error.message}`)
            }
        }
    }

    async function updateMode() {
        let counts = {}
        try {
            counts = JSON.parse(mockCounts || '{}')
        } catch (e) {
            counts = {}
        }
        const payload = {
            sessionId,
            listId: activeListId || null,
            mode,
            continuousBatchSize: batchSize,
            mockGroupOrder: mockOrder.split(',').map(s => s.trim()).filter(Boolean),
            mockGroupCounts: counts
        }
        await callApi('/api/modes', {
            method: 'PUT',
            headers: {'Content-Type': 'application/json'},
            body: JSON.stringify(payload)
        })
        setMockFinalRequested(false)
    }

    return (
        <div className="app">
            <div className="header">
                <div>
                    <div className="title">OMyPIc</div>
                    <div className="subtitle">Local speaking coach: realtime STT -> feedback -> TTS with rulebook +
                        wrong-note loop.
                    </div>
                </div>
                <div className="badge">Session {sessionId.slice(0, 8)}</div>
            </div>

            <div className="card-grid">
                <div className="card">
                    <h3>모델 & API Key</h3>
                    <div className="field">
                        <label>Provider</label>
                        <select value={provider} onChange={e => setProvider(e.target.value)}>
                            <option value="openai">OpenAI</option>
                            <option value="anthropic">Claude</option>
                            <option value="gemini">Gemini</option>
                        </select>
                    </div>
                    <div className="field">
                        <label>Model</label>
                        <select value={model} onChange={e => setModel(e.target.value)}>
                            {defaultModels[provider].map(item => (
                                <option key={item} value={item}>{item}</option>
                            ))}
                        </select>
                    </div>
                    <div className="field">
                        <label>API Key</label>
                        <input value={apiKeyInput} onChange={e => setApiKeyInput(e.target.value)}
                               placeholder="Paste your API key"/>
                    </div>
                    <button onClick={handleSaveApiKey}>저장 + 검증</button>
                    {apiKeyStatus && <div className="subtitle" style={{marginTop: '8px'}}>{apiKeyStatus}</div>}
                </div>

                <div className="card">
                    <h3>피드백 언어</h3>
                    <div className="row">
                        <button className={feedbackLang === 'ko' ? '' : 'secondary'}
                                onClick={() => setFeedbackLang('ko')}>한국어
                        </button>
                        <button className={feedbackLang === 'en' ? '' : 'secondary'}
                                onClick={() => setFeedbackLang('en')}>English
                        </button>
                    </div>
                    <div className="field" style={{marginTop: '12px'}}>
                        <label>Voice</label>
                        <select value={voice} onChange={e => setVoice(e.target.value)}>
                            {voices.map(v => (
                                <option key={v} value={v}>{v}</option>
                            ))}
                        </select>
                    </div>
                    <div className="subtitle" style={{marginTop: '8px'}}>
                        Realtime STT/TTS는 OpenAI key를 사용합니다.
                    </div>
                </div>

                <div className="card">
                    <h3>학습 모드</h3>
                    <div className="field">
                        <label>Mode</label>
                        <select value={mode} onChange={e => setMode(e.target.value)}>
                            <option value="IMMEDIATE">즉시 피드백</option>
                            <option value="CONTINUOUS">연속 발화</option>
                            <option value="MOCK_EXAM">모의고사</option>
                        </select>
                    </div>
                    {mode === 'CONTINUOUS' && (
                        <div className="field">
                            <label>N개 묶음 피드백</label>
                            <input type="number" min="1" max="10" value={batchSize}
                                   onChange={e => setBatchSize(Number(e.target.value))}/>
                        </div>
                    )}
                    {mode === 'MOCK_EXAM' && (
                        <>
                            <div className="field">
                                <label>그룹 순서 (쉼표 구분)</label>
                                <input value={mockOrder} onChange={e => setMockOrder(e.target.value)}
                                       placeholder="A,B,C"/>
                            </div>
                            <div className="field">
                                <label>그룹별 횟수 JSON</label>
                                <input value={mockCounts} onChange={e => setMockCounts(e.target.value)}
                                       placeholder='{"A":2,"B":3}'/>
                            </div>
                        </>
                    )}
                    <button onClick={updateMode}>모드 적용</button>
                </div>
            </div>

            <div className="card-grid">
                <div className="card">
                    <h3>Realtime STT</h3>
                    <div className="row">
                        <button onClick={startRecording} disabled={recording}>녹음 시작</button>
                        <button className="secondary" onClick={stopRecording} disabled={!recording}>녹음 정지</button>
                    </div>
                    <div className="subtitle" style={{marginTop: '8px'}}>
                        연결 상태: {realtimeConnected ? 'Connected' : 'Disconnected'}
                    </div>
                    {partialTranscript && (
                        <div className="field" style={{marginTop: '8px'}}>
                            <label>중간 인식</label>
                            <div>{partialTranscript}</div>
                        </div>
                    )}
                    <div className="field" style={{marginTop: '12px'}}>
                        <label>최종 텍스트</label>
                        <textarea value={userText} onChange={e => setUserText(e.target.value)}
                                  placeholder="녹음 종료 후 STT 결과가 채워집니다"/>
                    </div>
                    {feedback && (
                        <div className="feedback-block">
                            <div className="section-title">요약</div>
                            <div>{feedback.summary}</div>
                            <div className="section-title" style={{marginTop: '8px'}}>교정 포인트</div>
                            <ul className="list">
                                {feedback.correctionPoints.map((point, idx) => (
                                    <li key={idx}>{point}</li>
                                ))}
                            </ul>
                            <div className="section-title" style={{marginTop: '8px'}}>개선 예시 답변</div>
                            <div>{feedback.exampleAnswer}</div>
                            {feedback.rulebookEvidence && feedback.rulebookEvidence.length > 0 && (
                                <>
                                    <div className="section-title" style={{marginTop: '8px'}}>룰북 근거</div>
                                    <ul className="list">
                                        {feedback.rulebookEvidence.map((item, idx) => (
                                            <li key={idx}>{item}</li>
                                        ))}
                                    </ul>
                                </>
                            )}
                        </div>
                    )}
                </div>

                <div className="card">
                    <h3>룰북 관리</h3>
                    <div className="field">
                        <label>룰북 업로드 (.md)</label>
                        <input type="file" accept=".md"
                               onChange={e => e.target.files[0] && uploadRulebook(e.target.files[0])}/>
                    </div>
                    <div className="subtitle">Notion -> .md 로 추출 후 업로드하세요. 긴 문서는 분할 업로드를 권장합니다.</div>
                    <ul className="list" style={{marginTop: '12px'}}>
                        {rulebooks.map(book => (
                            <li key={book.id}>
                                <div className="row" style={{justifyContent: 'space-between'}}>
                                    <div>{book.filename}</div>
                                    <div className="row">
                                        <button className="secondary"
                                                onClick={() => toggleRulebook(book.id, !book.enabled)}>{book.enabled ? '비활성' : '활성'}</button>
                                        <button className="secondary" onClick={() => deleteRulebook(book.id)}>삭제
                                        </button>
                                    </div>
                                </div>
                            </li>
                        ))}
                    </ul>
                </div>

                <div className="card">
                    <h3>질문 리스트</h3>
                    <div className="field">
                        <label>새 리스트 이름</label>
                        <input value={newListName} onChange={e => setNewListName(e.target.value)}/>
                    </div>
                    <div className="row">
                        <button onClick={createList}>리스트 생성</button>
                        <button className="secondary" onClick={deleteList} disabled={!activeListId}>리스트 삭제</button>
                    </div>
                    <div className="field" style={{marginTop: '12px'}}>
                        <label>리스트 선택</label>
                        <select value={activeListId} onChange={e => setActiveListId(e.target.value)}>
                            {questionLists.map(list => (
                                <option key={list.id} value={list.id}>{list.name}</option>
                            ))}
                        </select>
                    </div>
                    <div className="field">
                        <label>질문 추가</label>
                        <input value={newQuestion} onChange={e => setNewQuestion(e.target.value)} placeholder="질문"/>
                    </div>
                    <div className="field">
                        <label>그룹 (모의고사용)</label>
                        <input value={newGroup} onChange={e => setNewGroup(e.target.value)} placeholder="Group A"/>
                    </div>
                    <button onClick={addQuestion}>질문 추가</button>
                    {activeQuestionList && activeQuestionList.questions && activeQuestionList.questions.length > 0 && (
                        <ul className="list" style={{marginTop: '12px'}}>
                            {activeQuestionList.questions.map(item => (
                                <li key={item.id}>
                                    {editingQuestionId === item.id ? (
                                        <>
                                            <div className="field">
                                                <label>질문</label>
                                                <input value={editingQuestionText}
                                                       onChange={e => setEditingQuestionText(e.target.value)}/>
                                            </div>
                                            <div className="field">
                                                <label>그룹</label>
                                                <input value={editingQuestionGroup}
                                                       onChange={e => setEditingQuestionGroup(e.target.value)}/>
                                            </div>
                                            <div className="row">
                                                <button onClick={saveEditedQuestion}>저장</button>
                                                <button className="secondary" onClick={cancelEditQuestion}>취소</button>
                                            </div>
                                        </>
                                    ) : (
                                        <div className="row" style={{justifyContent: 'space-between'}}>
                                            <div>
                                                <div>{item.text}</div>
                                                {item.group && <div className="subtitle">그룹: {item.group}</div>}
                                            </div>
                                            <div className="row">
                                                <button className="secondary"
                                                        onClick={() => startEditQuestion(item)}>수정
                                                </button>
                                                <button className="secondary"
                                                        onClick={() => removeQuestion(item.id)}>삭제
                                                </button>
                                            </div>
                                        </div>
                                    )}
                                </li>
                            ))}
                        </ul>
                    )}
                    <div className="row" style={{marginTop: '12px'}}>
                        <button className="secondary" onClick={nextQuestion}>다음 질문</button>
                        {currentQuestion && (
                            <div className="badge">
                                {currentQuestion.mockExamCompleted
                                    ? '모의고사 질문 소진'
                                    : (currentQuestion.text || '질문 없음')}
                            </div>
                        )}
                    </div>
                </div>

                <div className="card">
                    <h3>오답노트</h3>
                    <ul className="list">
                        {wrongNotes.map(note => (
                            <li key={note.id}>
                                <div><strong>{note.shortSummary}</strong></div>
                                <div className="subtitle">{note.pattern} · {note.count}회</div>
                            </li>
                        ))}
                    </ul>
                </div>
            </div>
        </div>
    )
}
