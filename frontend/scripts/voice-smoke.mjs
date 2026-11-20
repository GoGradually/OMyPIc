#!/usr/bin/env node
// Voice smoke check: loads the three on-device speech models in a real WebKit browser through the
// app's own client modules (the same workers the app starts) and measures them one at a time.
//   1. Kokoro (English TTS): load, then one English sentence.
//   2. Supertonic 2 (Korean TTS): load, then one Korean sentence.
//   3. Whisper (STT): load, then transcribe the English audio from step 1; the text must contain
//      the expected word.
// It talks W3C WebDriver over fetch (no npm dependencies) and starts the driver itself:
// safaridriver on macOS (Safari runs on the same system WebKit as the desktop app's WKWebView),
// WebKitWebDriver + MiniBrowser on Linux (WebKitGTK; run it under Xvfb or another X display).
// Needs the backend (model files) and the Vite dev server (`npm run dev:web`) running.
//
//   node scripts/voice-smoke.mjs --driver safari|webkitgtk [--app-url http://127.0.0.1:5173]
//        [--out result.json] [--max-load-seconds 120] [--max-synthesis-seconds 60] [--max-rss-mb 4096]
//        [--release-check]
//
// With --release-check it then releases the three models the way the app does when they are not
// needed (the workers are terminated), records the web process memory before and after, loads and
// runs all three again (the reload times) and releases them once more. The release must free the
// workers; the memory figures are reported, not judged.
//
// Exit code 0 when every step passes within the limits, 1 otherwise (the reason is printed).
import {spawn, execFile} from 'node:child_process'
import fs from 'node:fs'
import net from 'node:net'
import path from 'node:path'
import {parseArgs} from 'node:util'

const ENGLISH_SENTENCE = 'I like to walk in the park every weekend.'
const EXPECTED_WORD = 'park'
const KOREAN_SENTENCE = '전반적으로 내용 전달은 좋았습니다.'
// Speech is far above this; an all-zero or near-silent buffer is a broken synthesis.
const MIN_RMS = 0.01

const CLIENTS = {
    tts: '/src/features/voice/local/localTts.js',
    koTts: '/src/features/voice/local/localKoTts.js',
    stt: '/src/features/voice/local/localStt.js',
    http: '/src/shared/api/http.js'
}
const WORKERS = [
    '/src/features/voice/local/localTtsWorker.js',
    '/src/features/voice/local/localKoTtsWorker.js',
    '/src/features/voice/local/localSttWorker.js'
]
const DEFAULT_MINIBROWSER = '/usr/lib/x86_64-linux-gnu/webkit2gtk-4.1/MiniBrowser'
// Time for the dependency warm-up (the first run makes Vite bundle the speech libraries and reload).
const WARMUP_SECONDS = 240
const WEBDRIVER_CALL_MS = 30000

const {values: opts} = parseArgs({
    options: {
        driver: {type: 'string'},
        'app-url': {type: 'string', default: 'http://127.0.0.1:5173'},
        out: {type: 'string'},
        'max-load-seconds': {type: 'string', default: '120'},
        'max-synthesis-seconds': {type: 'string', default: '60'},
        'max-rss-mb': {type: 'string', default: '4096'},
        'browser-binary': {type: 'string', default: DEFAULT_MINIBROWSER},
        'release-check': {type: 'boolean', default: false}
    }
})
if (!['safari', 'webkitgtk'].includes(opts.driver)) {
    console.error('usage: node scripts/voice-smoke.mjs --driver safari|webkitgtk [--app-url URL] [--out FILE] [--max-load-seconds N] [--max-synthesis-seconds N] [--max-rss-mb N] [--release-check]')
    process.exit(2)
}
const appUrl = new URL(opts['app-url']).href
const limits = {
    loadSeconds: Number(opts['max-load-seconds']),
    synthesisSeconds: Number(opts['max-synthesis-seconds']),
    rssMb: Number(opts['max-rss-mb'])
}
for (const [name, value] of Object.entries(limits)) {
    if (!(value > 0)) {
        console.error(`invalid limit ${name}: ${value}`)
        process.exit(2)
    }
}

const started = Date.now()
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms))
const elapsed = () => ((Date.now() - started) / 1000).toFixed(0).padStart(4) + 's'
const log = (...args) => console.log(elapsed(), ...args)

class SmokeFailure extends Error {}
const fail = (message) => {
    throw new SmokeFailure(message)
}

const result = {
    driver: opts.driver,
    appUrl,
    limits,
    startedAt: new Date(started).toISOString(),
    browser: {},
    steps: [],
    memory: {peakTotalMb: 0, peakProcessMb: 0, samples: 0, process: opts.driver === 'safari' ? 'com.apple.WebKit.WebContent' : 'WebKitWebProcess'},
    ok: false,
    failure: null
}

// ---------------------------------------------------------------- driver process

let driver = null
let driverOutput = ''
let driverExit = null

async function freePort() {
    return new Promise((resolve, reject) => {
        const server = net.createServer()
        server.once('error', reject)
        server.listen(0, '127.0.0.1', () => {
            const {port} = server.address()
            server.close(() => resolve(port))
        })
    })
}

function startDriver(port) {
    let command, args, env
    if (opts.driver === 'safari') {
        command = 'safaridriver'
        args = ['-p', String(port)]
        env = process.env
    } else {
        command = 'WebKitWebDriver'
        args = [`--port=${port}`]
        // Keep the browser's local requests off any HTTP proxy in the environment. The sandbox
        // cannot start in an unprivileged container; this page is local test content only.
        const noProxy = ['127.0.0.1', 'localhost', process.env.NO_PROXY, process.env.no_proxy].filter(Boolean).join(',')
        env = {...process.env, NO_PROXY: noProxy, no_proxy: noProxy, WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS: '1'}
    }
    driver = spawn(command, args, {env, stdio: ['ignore', 'pipe', 'pipe'], detached: true})
    const keep = (chunk) => {
        driverOutput = (driverOutput + chunk.toString()).slice(-4000)
    }
    driver.stdout.on('data', keep)
    driver.stderr.on('data', keep)
    driver.on('error', (error) => {
        driverExit = `could not start ${command}: ${error.message}`
    })
    driver.on('exit', (code, signal) => {
        driverExit = driverExit || `${command} exited (code ${code}, signal ${signal})`
    })
}

// ---------------------------------------------------------------- WebDriver over fetch

let base = ''
let sessionId = null

async function wd(method, urlPath, body, timeoutMs = WEBDRIVER_CALL_MS) {
    let response
    try {
        response = await fetch(`${base}${urlPath}`, {
            method,
            headers: {'Content-Type': 'application/json; charset=utf-8'},
            body: body === undefined ? undefined : JSON.stringify(body),
            signal: AbortSignal.timeout(timeoutMs)
        })
    } catch (error) {
        const reason = error.name === 'TimeoutError' ? `no answer in ${timeoutMs / 1000} s` : error.cause?.message || error.message
        throw new Error(`WebDriver ${method} ${urlPath}: ${reason}`)
    }
    const text = await response.text()
    let json = null
    try {
        json = JSON.parse(text)
    } catch {
        // not JSON; reported below
    }
    if (!response.ok || json === null) {
        const value = json?.value
        const detail = value?.message ? `${value.error}: ${value.message}` : text.slice(0, 500)
        throw new Error(`WebDriver ${method} ${urlPath} -> HTTP ${response.status} ${detail}`)
    }
    return json.value
}

const S = (suffix) => `/session/${sessionId}${suffix}`
// Synchronous scripts only (no promises are returned to the driver); async work runs as page jobs.
const exec = (script, args = [], timeoutMs) => wd('POST', S('/execute/sync'), {script, args}, timeoutMs)

async function waitForDriver() {
    const deadline = Date.now() + 30000
    while (Date.now() < deadline) {
        if (driverExit) {
            fail(`${driverExit}\n${driverOutput}`)
        }
        try {
            const status = await wd('GET', '/status', undefined, 2000)
            if (status?.ready !== false) {
                return
            }
        } catch {
            // not listening yet
        }
        await sleep(250)
    }
    fail(`the WebDriver server did not answer on ${base} within 30 s\n${driverOutput}`)
}

function capabilities() {
    if (opts.driver === 'safari') {
        return {browserName: 'safari'}
    }
    return {
        browserName: 'MiniBrowser',
        'webkitgtk:browserOptions': {binary: opts['browser-binary'], args: ['--automation']}
    }
}

async function createSession() {
    // A fresh Safari sometimes refuses the first session while it is still starting up.
    const attempts = opts.driver === 'safari' ? 3 : 1
    for (let attempt = 1; ; attempt++) {
        try {
            const value = await wd('POST', '/session', {capabilities: {alwaysMatch: capabilities()}}, 90000)
            sessionId = value.sessionId
            result.browser.name = value.capabilities?.browserName
            result.browser.version = value.capabilities?.browserVersion
            result.browser.platform = value.capabilities?.platformName
            return
        } catch (error) {
            if (attempt >= attempts) {
                fail(`could not create a WebDriver session: ${error.message}${driverOutput ? `\ndriver output:\n${driverOutput}` : ''}`)
            }
            log(`session attempt ${attempt} failed (${error.message}); retrying`)
            await sleep(5000)
        }
    }
}

// ---------------------------------------------------------------- memory sampling

let currentStep = 'startup'
const knownWebPids = new Set()
let rssExceeded = null

function readLinuxWebProcesses() {
    // Only the web processes under this run's driver, so another WebKit browser on the machine
    // does not count.
    const parent = new Map()
    const cmdline = new Map()
    for (const pid of fs.readdirSync('/proc')) {
        if (!/^\d+$/.test(pid)) continue
        try {
            const stat = fs.readFileSync(`/proc/${pid}/stat`, 'utf8')
            parent.set(Number(pid), Number(stat.slice(stat.lastIndexOf(')') + 2).split(' ')[1]))
            cmdline.set(Number(pid), fs.readFileSync(`/proc/${pid}/cmdline`, 'utf8'))
        } catch {
            // the process ended
        }
    }
    const underDriver = (pid) => {
        for (let p = pid, hops = 0; p > 1 && hops < 20; p = parent.get(p), hops++) {
            if (p === driver?.pid) return true
        }
        return false
    }
    const list = []
    for (const [pid, cmd] of cmdline) {
        if (!underDriver(pid)) continue
        knownWebPids.add(pid)
        if (!cmd.includes('WebKitWebProcess')) continue
        try {
            const status = fs.readFileSync(`/proc/${pid}/status`, 'utf8')
            list.push(Number(/VmRSS:\s+(\d+)/.exec(status)?.[1] || 0) / 1024)
        } catch {
            // the process ended
        }
    }
    return list
}

function readMacWebProcesses() {
    return new Promise((resolve) => {
        execFile('ps', ['-axo', 'rss=,comm='], {timeout: 5000, maxBuffer: 4 * 1024 * 1024}, (error, stdout) => {
            if (error) {
                resolve(null)
                return
            }
            const list = []
            for (const line of stdout.split('\n')) {
                const match = /^\s*(\d+)\s+(.*)$/.exec(line)
                if (match && match[2].includes('com.apple.WebKit.WebContent')) {
                    list.push(Number(match[1]) / 1024)
                }
            }
            resolve(list)
        })
    })
}

// For the report when the expected process name never matched.
function listWebKitProcesses() {
    return new Promise((resolve) => {
        execFile('ps', ['-axo', 'rss=,comm='], {timeout: 5000, maxBuffer: 4 * 1024 * 1024}, (error, stdout) => {
            resolve(error ? [] : stdout.split('\n').filter((line) => /webkit|safari/i.test(line)).map((line) => line.trim()).slice(0, 20))
        })
    })
}

let sampling = false
async function sampleMemory() {
    if (sampling) return
    sampling = true
    try {
        const list = opts.driver === 'safari' ? await readMacWebProcesses() : readLinuxWebProcesses()
        if (!list) return
        const total = Math.round(list.reduce((sum, mb) => sum + mb, 0))
        const largest = Math.round(Math.max(0, ...list))
        result.memory.samples += 1
        result.memory.maxProcesses = Math.max(result.memory.maxProcesses || 0, list.length)
        result.memory.peakTotalMb = Math.max(result.memory.peakTotalMb, total)
        result.memory.peakProcessMb = Math.max(result.memory.peakProcessMb, largest)
        const step = result.steps.find((s) => s.name === currentStep)
        if (step) step.peakRssMb = Math.max(step.peakRssMb || 0, total)
        if (total > limits.rssMb && !rssExceeded) {
            rssExceeded = `web content memory reached ${total} MB during "${currentStep}", above --max-rss-mb ${limits.rssMb}`
        }
    } finally {
        sampling = false
    }
}

// ---------------------------------------------------------------- the page side

// Installed on every page load. Jobs run async in the page; Node polls their state with short
// synchronous scripts, so a hung page shows up as missed polls, never as a WebDriver call that
// waits forever. `mark` tells Node whether the page reloaded since the agent was installed.
const AGENT = `
const [mark, clients, workers] = arguments
const agent = {mark, jobs: {}, data: {}}
window.__voiceSmoke = agent
const toPcm16 = (samples) => {
    const pcm = new Int16Array(samples.length)
    for (let i = 0; i < samples.length; i++) {
        const v = Math.max(-1, Math.min(1, samples[i]))
        pcm[i] = v < 0 ? v * 32768 : v * 32767
    }
    return new Uint8Array(pcm.buffer)
}
const audioStats = (samples, sampleRate) => {
    let sum = 0
    let peak = 0
    for (let i = 0; i < samples.length; i++) {
        sum += samples[i] * samples[i]
        peak = Math.max(peak, Math.abs(samples[i]))
    }
    return {samples: samples.length, sampleRate, audioSeconds: samples.length / sampleRate, rms: samples.length ? Math.sqrt(sum / samples.length) : 0, peak}
}
const seconds = (from) => (performance.now() - from) / 1000
const run = (name, work) => {
    const job = agent.jobs[name] = {state: 'running', phase: 'start'}
    Promise.resolve().then(() => work(job)).then((value) => {
        job.value = value
        job.state = 'done'
    }, (error) => {
        job.error = String((error && error.message) || error)
        job.state = 'failed'
    })
}
// Fetch each worker module and the app modules it imports, so Vite transforms them and bundles
// every dependency they need now, instead of reloading the page in the middle of a measurement.
const warmup = async () => {
    await Promise.all(Object.values(clients).map((p) => import(p)))
    const seen = new Set()
    const bad = []
    const visit = async (url) => {
        if (seen.has(url)) return
        seen.add(url)
        const response = await fetch(url, {cache: 'no-store'})
        if (!response.ok) {
            bad.push(new URL(url).pathname + ' HTTP ' + response.status)
            return
        }
        const text = await response.text()
        if (new URL(url).pathname.startsWith('/node_modules/')) return
        const specs = []
        const re = /(?:\\bfrom\\s*|\\bimport\\s*\\(?\\s*)["']([^"']+)["']/g
        let m
        while ((m = re.exec(text))) specs.push(m[1])
        await Promise.all(specs.map((spec) => {
            const next = new URL(spec, url)
            const p = next.pathname
            return next.origin === location.origin && (p.startsWith('/src/') || p.startsWith('/node_modules/.vite/deps/')) ? visit(next.href) : null
        }))
    }
    await Promise.all(workers.map((w) => visit(new URL(w, location.href).href)))
    return {modules: seen.size, bad}
}
const backendCheck = async () => {
    const http = await import(clients.http)
    const backend = await http.getBackendUrl()
    const controller = new AbortController()
    const timer = setTimeout(() => controller.abort(), 10000)
    try {
        const response = await fetch(backend + '/api/meta/models', {signal: controller.signal})
        return {backend, status: response.status}
    } catch (error) {
        throw new Error('the backend at ' + backend + ' is not reachable from the page (' + ((error && error.message) || error) + ')')
    } finally {
        clearTimeout(timer)
    }
}
const steps = {
    warmup: (job) => warmup(),
    backend: (job) => backendCheck(),
    kokoro: async (job, sentence) => {
        const tts = await import(clients.tts)
        let t = performance.now()
        job.phase = 'load'
        await tts.loadLocalTts()
        const loadSeconds = seconds(t)
        job.phase = 'run'
        t = performance.now()
        const out = await tts.synthesizeEnglish(sentence)
        const runSeconds = seconds(t)
        agent.data.english = out
        return {loadSeconds, runSeconds, ...audioStats(out.samples, out.sampleRate)}
    },
    supertonic: async (job, sentence) => {
        const ko = await import(clients.koTts)
        let t = performance.now()
        job.phase = 'load'
        await ko.loadLocalKoTts()
        const loadSeconds = seconds(t)
        job.phase = 'run'
        t = performance.now()
        const out = await ko.synthesizeKorean(sentence)
        return {loadSeconds, runSeconds: seconds(t), ...audioStats(out.samples, out.sampleRate)}
    },
    whisper: async (job) => {
        const english = agent.data.english
        if (!english) throw new Error('no English audio to transcribe (the Kokoro step did not produce any)')
        const stt = await import(clients.stt)
        let t = performance.now()
        job.phase = 'load'
        await stt.loadLocalStt()
        const loadSeconds = seconds(t)
        job.phase = 'run'
        t = performance.now()
        // The client takes 16-bit PCM at the capture rate and resamples it to 16 kHz itself.
        const transcript = await stt.transcribePcm16(toPcm16(english.samples), english.sampleRate)
        return {loadSeconds, runSeconds: seconds(t), transcript, ...audioStats(english.samples, english.sampleRate)}
    }
}
// --release-check: what the app does with models it does not need. The worker count is the
// clients' own view: a released client has no worker.
steps.release = async (job) => {
    const [tts, ko, stt] = await Promise.all([import(clients.tts), import(clients.koTts), import(clients.stt)])
    return {kokoro: tts.releaseLocalTts(), supertonic: ko.releaseLocalKoTts(), whisper: stt.releaseLocalStt()}
}
steps['kokoro-reload'] = steps.kokoro
steps['supertonic-reload'] = steps.supertonic
steps['whisper-reload'] = steps.whisper
agent.start = (name, arg) => run(name, (job) => steps[name](job, arg))
window.addEventListener('error', (e) => { agent.lastError = String(e.message) })
window.addEventListener('unhandledrejection', (e) => { agent.lastError = 'unhandled: ' + String((e.reason && e.reason.message) || e.reason) })
return true
`

const POLL = `
const [mark, name] = arguments
const agent = window.__voiceSmoke
if (!agent || agent.mark !== mark) return {reloaded: true}
const job = agent.jobs[name]
return job ? {state: job.state, phase: job.phase, error: job.error || null, value: job.value || null, lastError: agent.lastError || null} : {state: 'missing'}
`

let mark = ''

async function openPage() {
    await wd('POST', S('/url'), {url: appUrl}, 90000)
    const deadline = Date.now() + 30000
    while ((await exec('return document.readyState').catch(() => '')) !== 'complete') {
        if (Date.now() > deadline) fail(`the page at ${appUrl} did not finish loading within 30 s`)
        await sleep(250)
    }
    mark = `m${Date.now()}${Math.random().toString(36).slice(2)}`
    await exec(AGENT, [mark, CLIENTS, WORKERS])
}

async function pageIsCurrent() {
    try {
        return !(await exec(POLL, [mark, '-'], 10000)).reloaded
    } catch {
        return false
    }
}

// Runs one page job and waits for it. phaseLimits: {phase: seconds}; every phase has a deadline,
// so a hang ends as a failure that names the step and the phase.
async function runJob(name, phaseLimits, arg) {
    await exec(`window.__voiceSmoke.start(arguments[0], arguments[1]); return true`, [name, arg ?? null])
    let phase = null
    let phaseStarted = Date.now()
    let lastPollError = null
    while (true) {
        await sleep(500)
        if (rssExceeded) fail(rssExceeded)
        let state
        try {
            state = await exec(POLL, [mark, name], 15000)
            lastPollError = null
        } catch (error) {
            lastPollError = error.message
            state = null
        }
        if (state?.reloaded) fail(`the page reloaded during "${name}" (phase ${phase}); a Vite dependency reload or a crashed web process`)
        if (state && state.phase !== phase) {
            phase = state.phase
            phaseStarted = Date.now()
        }
        if (state?.state === 'done') return state.value
        if (state?.state === 'failed') fail(`"${name}" failed in phase ${phase}: ${state.error}${state.lastError ? ` (page error: ${state.lastError})` : ''}`)
        const limit = phaseLimits[phase] ?? phaseLimits.default
        if ((Date.now() - phaseStarted) / 1000 > limit) {
            fail(`"${name}" did not finish phase ${phase} within ${limit} s${lastPollError ? `; the page stopped answering: ${lastPollError}` : ''}`)
        }
    }
}

async function warmUp() {
    const deadline = Date.now() + WARMUP_SECONDS * 1000
    let lastError = ''
    for (let attempt = 1; Date.now() < deadline; attempt++) {
        await openPage()
        let warm
        try {
            warm = await runJob('warmup', {default: Math.max(10, (deadline - Date.now()) / 1000)})
        } catch (error) {
            // While Vite bundles new dependencies it reloads the page. WebKitGTK shows that as a
            // replaced page; Safari first fails the requests that were in flight ("Load failed").
            // Either way the next attempt starts from a fresh page, until the warm-up budget ends.
            lastError = error.message
            log(`warm-up ${attempt}: ${/reloaded/.test(lastError) ? 'the page reloaded (Vite bundled new dependencies)' : lastError}; again`)
            await sleep(2000)
            continue
        }
        // Stable means: every module answered and the page did not reload for 5 s afterwards.
        let stable = warm.bad.length === 0
        for (let i = 0; stable && i < 10; i++) {
            await sleep(500)
            stable = await pageIsCurrent()
        }
        log(`warm-up ${attempt}: ${warm.modules} modules${warm.bad.length ? `, not ready: ${warm.bad.join(', ')}` : ''}${stable ? ', page stable' : ', page not stable yet'}`)
        if (stable) return
        await sleep(2000)
    }
    fail(`the dev server did not settle within ${WARMUP_SECONDS} s (Vite kept reloading or failing modules${lastError ? `; last error: ${lastError}` : ''}); see the Vite log`)
}

// ---------------------------------------------------------------- the run

function checkAudio(step) {
    if (!(step.samples > 0) || !(step.sampleRate > 0)) fail(`${step.name}: no audio (samples ${step.samples}, rate ${step.sampleRate})`)
    if (!(step.rms >= MIN_RMS)) fail(`${step.name}: the audio is silent (RMS ${step.rms?.toFixed(4)} < ${MIN_RMS})`)
}

function checkTimes(step) {
    if (step.loadSeconds > limits.loadSeconds) fail(`${step.name}: load took ${step.loadSeconds.toFixed(1)} s, above --max-load-seconds ${limits.loadSeconds}`)
    if (step.runSeconds > limits.synthesisSeconds) fail(`${step.name}: ${step.kind} took ${step.runSeconds.toFixed(1)} s, above --max-synthesis-seconds ${limits.synthesisSeconds}`)
}

async function measure(name, kind, arg) {
    currentStep = name
    const step = {name, kind, ok: false}
    result.steps.push(step)
    log(`${name}: load + ${kind}`)
    const value = await runJob(name, {start: 30, load: limits.loadSeconds + 5, run: limits.synthesisSeconds + 5}, arg)
    Object.assign(step, value)
    checkTimes(step)
    checkAudio(step)
    step.ok = true
    log(`${name}: load ${step.loadSeconds.toFixed(1)} s, ${kind} ${step.runSeconds.toFixed(1)} s`)
    return step
}

async function currentRssMb() {
    const list = opts.driver === 'safari' ? await readMacWebProcesses() : readLinuxWebProcesses()
    return list ? Math.round(list.reduce((sum, mb) => sum + mb, 0)) : null
}

// Memory once it has settled: the lowest of a few samples over some seconds (a terminated
// worker's memory is returned shortly after, not at once).
async function settledRssMb(seconds = 8) {
    let lowest = null
    for (let i = 0; i < seconds; i++) {
        await sleep(1000)
        const mb = await currentRssMb()
        if (mb !== null) lowest = lowest === null ? mb : Math.min(lowest, mb)
    }
    return lowest
}

async function releaseModels(label) {
    currentStep = label
    const released = await runJob('release', {default: 30})
    if (!released.kokoro || !released.supertonic || !released.whisper) {
        fail(`${label}: not every model was released (${JSON.stringify(released)})`)
    }
    return released
}

async function releaseCheck() {
    const check = {loadedMb: await settledRssMb(4)}
    result.release = check
    log(`release check: ${check.loadedMb} MB with the three models loaded`)
    await releaseModels('release')
    check.afterReleaseMb = await settledRssMb()
    log(`release check: ${check.afterReleaseMb} MB after the release`)
    const reload = {}
    for (const [name, kind, arg] of [['kokoro-reload', 'synthesis', ENGLISH_SENTENCE], ['supertonic-reload', 'synthesis', KOREAN_SENTENCE], ['whisper-reload', 'transcription']]) {
        const step = await measure(name, kind, arg)
        reload[name] = step.loadSeconds
    }
    check.reloadSeconds = reload
    check.reloadedMb = await settledRssMb(4)
    log(`release check: ${check.reloadedMb} MB after the reload`)
    await releaseModels('release-again')
    check.afterSecondReleaseMb = await settledRssMb()
    log(`release check: ${check.afterSecondReleaseMb} MB after the second release`)
}

async function main() {
    try {
        await fetch(appUrl, {signal: AbortSignal.timeout(10000)})
    } catch (error) {
        fail(`the app is not reachable at ${appUrl} (${error.cause?.message || error.message}); start it with npm run dev:web`)
    }
    const port = await freePort()
    base = `http://127.0.0.1:${port}`
    startDriver(port)
    await waitForDriver()
    log(`${opts.driver} WebDriver on port ${port}`)
    await createSession()
    log(`session ${sessionId} (${result.browser.name} ${result.browser.version ?? ''})`)
    const scriptMs = (limits.loadSeconds + limits.synthesisSeconds + 60) * 1000
    await wd('POST', S('/timeouts'), {script: scriptMs, pageLoad: 90000})

    currentStep = 'warmup'
    await warmUp()
    await openPage()
    result.browser.userAgent = await exec('return navigator.userAgent')
    const backend = await runJob('backend', {default: 20})
    log(`backend ${backend.backend} answered HTTP ${backend.status}`)
    if (backend.status !== 200) fail(`the backend at ${backend.backend} answered HTTP ${backend.status}`)

    await measure('kokoro', 'synthesis', ENGLISH_SENTENCE)
    await measure('supertonic', 'synthesis', KOREAN_SENTENCE)
    const whisper = await measure('whisper', 'transcription')
    const words = whisper.transcript.toLowerCase().replace(/[^a-z\s]/g, ' ').split(/\s+/)
    whisper.expectedWord = EXPECTED_WORD
    if (!words.includes(EXPECTED_WORD)) {
        whisper.ok = false
        fail(`whisper: the transcript "${whisper.transcript}" does not contain "${EXPECTED_WORD}"`)
    }
    if (opts['release-check']) {
        await releaseCheck()
    }
    currentStep = 'done'
    await sampleMemory()
    if (rssExceeded) fail(rssExceeded)
    if (!result.memory.maxProcesses) {
        result.memory.webkitProcesses = await listWebKitProcesses()
        fail(`no ${result.memory.process} process was found, so the memory limit was not checked (WebKit processes seen: ${result.memory.webkitProcesses.join('; ') || 'none'})`)
    }
    result.ok = true
}

// ---------------------------------------------------------------- cleanup and report

let cleaned = false
async function cleanup() {
    if (cleaned) return
    cleaned = true
    if (sessionId) {
        await wd('DELETE', S(''), undefined, 15000).catch((error) => log(`could not end the session: ${error.message}`))
    }
    if (driver && driver.exitCode === null && driver.signalCode === null) {
        const exited = new Promise((resolve) => driver.once('exit', resolve))
        try {
            process.kill(-driver.pid, 'SIGTERM')
        } catch {
            driver.kill('SIGTERM')
        }
        await Promise.race([exited, sleep(5000)])
        try {
            process.kill(-driver.pid, 'SIGKILL')
        } catch {
            // already gone
        }
    }
    // A browser process that left the driver's process group (Linux) is still known by pid.
    for (const pid of knownWebPids) {
        try {
            process.kill(pid, 'SIGKILL')
        } catch {
            // already gone
        }
    }
}

const fmt = (value, digits = 1) => (typeof value === 'number' ? value.toFixed(digits) : '-')

function report() {
    result.finishedAt = new Date().toISOString()
    result.wallSeconds = (Date.now() - started) / 1000
    const rows = [['step', 'load s', 'run s', 'samples', 'rate', 'audio s', 'rms', 'peak MB', 'ok']]
    for (const s of result.steps) {
        rows.push([s.name, fmt(s.loadSeconds), fmt(s.runSeconds), String(s.samples ?? '-'), String(s.sampleRate ?? '-'), fmt(s.audioSeconds, 2), fmt(s.rms, 3), String(s.peakRssMb ?? '-'), s.ok ? 'yes' : 'NO'])
    }
    const widths = rows[0].map((_, i) => Math.max(...rows.map((r) => r[i].length)))
    console.log('')
    for (const r of rows) console.log(r.map((cell, i) => (i === 0 ? cell.padEnd(widths[i]) : cell.padStart(widths[i]))).join('  '))
    const whisper = result.steps.find((s) => s.name === 'whisper')
    if (whisper?.transcript !== undefined) console.log(`whisper transcript: "${whisper.transcript}"`)
    if (result.release) {
        const r = result.release
        console.log(`release check: ${r.loadedMb ?? '-'} MB loaded, ${r.afterReleaseMb ?? '-'} MB after release, ${r.reloadedMb ?? '-'} MB after reload, ${r.afterSecondReleaseMb ?? '-'} MB after the second release`)
    }
    console.log(`peak ${result.memory.process} RSS: ${result.memory.peakTotalMb} MB total, ${result.memory.peakProcessMb} MB largest process (limit ${limits.rssMb} MB, ${result.memory.samples} samples)`)
    console.log(`browser: ${result.browser.name ?? '?'} ${result.browser.version ?? ''} ${result.browser.userAgent ?? ''}`)
    console.log(result.ok ? 'VOICE SMOKE CHECK PASSED' : `VOICE SMOKE CHECK FAILED: ${result.failure}`)
    if (opts.out) {
        fs.mkdirSync(path.dirname(path.resolve(opts.out)), {recursive: true})
        fs.writeFileSync(opts.out, JSON.stringify(result, null, 2) + '\n')
        console.log(`wrote ${opts.out}`)
    }
}

async function finish(code) {
    clearInterval(sampler)
    clearTimeout(watchdog)
    await cleanup()
    report()
    process.exit(code)
}

const sampler = setInterval(() => {
    sampleMemory().catch(() => {})
}, 1000)
// Last line of defense: whatever hangs, the run ends well before any CI limit.
const totalSeconds = WARMUP_SECONDS + (opts['release-check'] ? 6 : 3) * (limits.loadSeconds + limits.synthesisSeconds + 20) + (opts['release-check'] ? 180 : 120)
const watchdog = setTimeout(() => {
    result.failure = `the whole check exceeded ${totalSeconds} s (step "${currentStep}")`
    finish(1)
}, totalSeconds * 1000)
for (const signal of ['SIGINT', 'SIGTERM']) {
    process.on(signal, () => {
        result.failure = `interrupted (${signal}) during "${currentStep}"`
        finish(1)
    })
}

main().then(() => finish(0), (error) => {
    result.failure = error instanceof SmokeFailure ? error.message : String(error.stack || error)
    finish(1)
})
