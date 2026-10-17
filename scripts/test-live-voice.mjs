// Behaviour tests for web/static/live-voice.js — run with `node scripts/test-live-voice.mjs`.
//
// Loads the real script into a fresh vm context per scenario, with fake browser APIs (media
// devices, audio, canvas, a controllable clock) and a fake /ws/voice server, so the failure
// paths can be checked without a camera, a microphone or a Gemini key.
import { readFileSync } from 'node:fs';
import vm from 'node:vm';
import assert from 'node:assert/strict';

const SOURCE = readFileSync(new URL('../web/static/live-voice.js', import.meta.url), 'utf8');
const tick = (ms = 0) => new Promise((r) => setTimeout(r, ms));

function fakeTrack() {
  return { readyState: 'live', stop() { this.readyState = 'ended'; } };
}
function fakeStream() {
  const tracks = [fakeTrack()];
  return { getTracks: () => tracks };
}
const live = (streams) => streams.flatMap((s) => s.getTracks()).filter((t) => t.readyState === 'live').length;
const deviceError = (name) => Object.assign(new Error(name), { name });

/**
 * One page: `status` is what /api/voice/status answers; `media` answers getUserMedia (a function
 * returning a promise); the fake server answers each socket with `connected`, or with `refuse`,
 * or (`silent`) not at all.
 */
function page({ status, media, pageSession = 'page-session', refuse = null, silent = false } = {}) {
  const events = new EventTarget();
  const seen = [];
  const toasts = [];
  const sockets = [];
  const acquired = [];
  const store = new Map([['agentrix_session_id', pageSession]]);
  let now = 1_000_000;

  class FakeDate extends Date {
    static now() {
      return now;
    }
  }

  class FakeWS {
    static OPEN = 1;
    constructor(url) {
      this.url = url;
      this.readyState = 0;
      this.sent = [];
      sockets.push(this);
      setTimeout(() => {
        this.readyState = 1;
        this.onopen?.();
        if (silent) return;
        if (refuse) this.serverClose(refuse);
        else this.recv({ type: 'connected', session_id: 'voice-' + sockets.length, camera: true, output_rate: 24000 });
      }, 0);
    }
    recv(m) {
      this.onmessage?.({ data: JSON.stringify(m) });
    }
    send(d) {
      this.sent.push(typeof d === 'string' ? JSON.parse(d).type : 'pcm');
    }
    close() {
      if (this.readyState === 3) return;
      this.readyState = 3;
      setTimeout(() => this.onclose?.({ code: 1000 }), 0);
    }
    /** The server's last word, then its Close frame. */
    serverClose(m) {
      this.recv(m);
      if (this.readyState === 3) return;
      this.readyState = 3;
      this.onclose?.({ code: 1011, reason: m.reason });
    }
  }

  class FakeAudioContext {
    constructor() {
      this.state = 'running';
      this.currentTime = 0;
      this.destination = {};
    }
    createMediaStreamSource() {
      return { connect() {} };
    }
    createScriptProcessor() {
      return { connect() {}, disconnect() {} };
    }
    close() {
      return Promise.resolve();
    }
  }

  const ctx = {
    console: { warn() {}, info() {}, log() {} },
    setTimeout,
    clearTimeout,
    setInterval,
    clearInterval,
    CustomEvent,
    Date: FakeDate,
    WebSocket: FakeWS,
    AudioContext: FakeAudioContext,
    location: { protocol: 'http:', host: '127.0.0.1:9847' },
    sessionStorage: { getItem: (k) => store.get(k) ?? null, setItem: (k, v) => store.set(k, String(v)) },
    fetch: async () => ({ ok: true, json: async () => status }),
    navigator: {
      mediaDevices: {
        getUserMedia: async (c) => {
          const s = await media(c);
          acquired.push(s);
          return s;
        },
      },
    },
    document: {
      createElement: (tag) =>
        tag === 'video'
          ? { play: async () => {}, pause() {}, videoWidth: 640, videoHeight: 360, srcObject: null }
          : { getContext: () => ({ drawImage() {} }), toDataURL: () => 'data:image/jpeg;base64,AAAA' },
    },
    addEventListener: (n, f) => events.addEventListener(n, f),
    removeEventListener: (n, f) => events.removeEventListener(n, f),
    dispatchEvent: (e) => {
      seen.push([e.type, e.detail]);
      return events.dispatchEvent(e);
    },
    // The toast takes HTML; live-voice escapes what it shows, so decode it back for asserts.
    __AGENTRIX_UI__: {
      showSuzyCustom: (html) =>
        toasts.push(html.replace(/&(amp|lt|gt|quot|#39);/g, (_, e) => ({ amp: '&', lt: '<', gt: '>', quot: '"', '#39': "'" })[e])),
    },
    __AGENTRIX_LIVE__: { getSessionId: () => pageSession, whenSession: async () => pageSession },
  };
  ctx.window = ctx;
  vm.createContext(ctx);
  vm.runInContext(SOURCE, ctx);
  return {
    api: ctx.AgentrixLiveVoice,
    sockets,
    acquired,
    toasts,
    store,
    seen: (type) => seen.filter(([t]) => t === type).map(([, d]) => d),
    advance: (ms) => {
      now += ms;
    },
  };
}

const VOICE_OFF = { enabled: false, reason: 'Voice is off — no GOOGLE_API_KEY is configured.', camera: false, gestures: true };
const VOICE_ON = { enabled: true, camera: true, gestures: true };
const camera = () => Promise.resolve(fakeStream());
const slowCamera = () => new Promise((r) => setTimeout(() => r(fakeStream()), 30));

// 1. No key: the camera still runs (local gestures); no socket is opened; off stops the tracks.
{
  const p = page({ status: VOICE_OFF, media: camera });
  await p.api.probe();
  assert.equal(p.api.isEnabled(), false);
  assert.equal(p.api.isCameraEnabled(), true, 'the 📷 button works without voice');
  assert.equal(await p.api.startCamera({}), true);
  await tick(5);
  assert.equal(p.api.isCameraActive(), true);
  assert.equal(p.sockets.length, 0, 'no Live session without voice');
  assert.equal(p.seen('agentrix:camera')[0].active, true);
  assert.ok(p.seen('agentrix:camera')[0].stream, 'the detector gets the stream');
  p.api.stopCamera();
  assert.equal(live(p.acquired), 0, 'turning the camera off stops its tracks');
  assert.equal(p.seen('agentrix:camera').at(-1).active, false);
}

// 2. Permission denied / no device: a toast says so; nothing is left running.
for (const [name, words] of [['NotAllowedError', 'Camera permission denied'], ['NotFoundError', 'No camera found']]) {
  const p = page({ status: VOICE_OFF, media: () => Promise.reject(deviceError(name)) });
  await p.api.probe();
  assert.equal(await p.api.startCamera({}), false);
  assert.equal(p.api.lastFailure(), 'device');
  assert.match(p.toasts.at(-1), new RegExp(words));
  assert.equal(p.api.isCameraActive(), false);
}
{
  const p = page({ status: VOICE_ON, media: () => Promise.reject(deviceError('NotAllowedError')) });
  await p.api.probe();
  assert.equal(await p.api.startMic({}), false);
  assert.equal(p.api.lastFailure(), 'device', 'a denied mic does not fall back to dictation');
  assert.match(p.toasts.at(-1), /Microphone permission denied/);
  assert.equal(p.sockets.length, 0);
}

// 3. Double click: one device, one stream; a click while starting cancels and stops it.
{
  const p = page({ status: VOICE_OFF, media: slowCamera });
  await p.api.probe();
  const [a, b] = await Promise.all([p.api.startCamera({}), p.api.startCamera({})]);
  assert.deepEqual([a, b], [true, true]);
  assert.equal(p.acquired.length, 1, 'a second start while the first is pending reuses it');
  p.api.stopCamera();
  assert.equal(live(p.acquired), 0);

  const q = page({ status: VOICE_OFF, media: slowCamera });
  await q.api.probe();
  const started = q.api.startCamera({});
  assert.equal(q.api.isCameraStarting(), true);
  q.api.stopCamera(); // the second click, while the permission prompt is up
  assert.equal(await started, false);
  assert.equal(q.api.lastFailure(), 'cancelled');
  assert.equal(q.acquired.length, 1);
  assert.equal(live(q.acquired), 0, 'the stream that arrived after the cancel is stopped');
  assert.equal(q.api.isCameraActive(), false);
}
{
  const p = page({ status: VOICE_ON, media: () => new Promise((r) => setTimeout(() => r(fakeStream()), 30)) });
  await p.api.probe();
  const [a, b] = await Promise.all([p.api.startMic({}), p.api.startMic({})]);
  assert.deepEqual([a, b], [true, true]);
  assert.equal(p.acquired.length, 1, 'one microphone stream');
  assert.equal(p.sockets.length, 1, 'one Live session');
  p.api.stopMic();
  assert.equal(live(p.acquired), 0);
}

// 4. Voice on: the camera joins the page's session and sends frames while the session is open.
{
  const p = page({ status: VOICE_ON, media: camera });
  await p.api.probe();
  assert.equal(await p.api.startCamera({}), true);
  await tick(5);
  assert.equal(p.sockets.length, 1);
  assert.match(p.sockets[0].url, /session_id=page-session/, 'the voice socket carries the page session');
  assert.equal(p.store.get('agentrix_session_id'), 'page-session', 'the page keeps its own session');
  assert.equal(p.api.isCameraLive(), true);
  await tick(1100);
  assert.ok(p.sockets[0].sent.includes('frame'), 'frames go to Gemini while the session is open');
  p.api.stopCamera();
  await tick(5);
  assert.equal(p.sockets[0].readyState, 3, 'the last input off closes the session');
}

// 5. Gemini rejects the key mid-session: toast, voice off, the camera keeps running locally.
{
  const p = page({ status: VOICE_ON, media: camera });
  await p.api.probe();
  await p.api.startCamera({});
  await tick(5);
  const message = 'Voice is off — Gemini rejected the API key. Fix GOOGLE_API_KEY and restart the server.';
  p.sockets[0].serverClose({ type: 'closed', reason: 'key_rejected', message });
  await tick(5);
  assert.equal(p.toasts.at(-1), message);
  assert.equal(p.api.isEnabled(), false);
  assert.equal(p.api.isActive(), false, 'the dead socket is dropped');
  assert.equal(p.api.isCameraActive(), true, 'local gestures keep the camera');
  assert.equal(p.api.isCameraLive(), false);
  assert.equal(p.seen('agentrix:camera-live').at(-1).live, false);
  assert.equal(p.seen('agentrix:voice-status').at(-1).enabled, false);
  assert.equal(await p.api.speakText('Hello'), false, 'no new session with a rejected key');
  assert.equal(p.sockets.length, 1);
}

// 6. A refused session (voice became unavailable) fails the start and says why.
{
  const message = 'Voice is off — Gemini rejected the API key (API key not valid).';
  const p = page({ status: VOICE_ON, media: camera, refuse: { type: 'error', reason: 'voice_unavailable', message } });
  await p.api.probe();
  assert.equal(await p.api.startMic({}), false);
  assert.equal(p.api.lastFailure(), 'session');
  assert.equal(live(p.acquired), 0);
  assert.equal(p.toasts.at(-1), message);
  assert.equal(p.api.isEnabled(), false);
}

// 7. Gemini's time limit: the camera re-links quietly; a "limit" right after connecting does not
//    loop; speakText after a closed session opens a new one.
{
  const p = page({ status: VOICE_ON, media: camera });
  await p.api.probe();
  await p.api.startCamera({});
  await tick(5);
  p.advance(120_000);
  const limit = { type: 'closed', reason: 'session_limit', message: "Suzy's live session reached its time limit — tap 🎤 or 📷 to start a new one." };
  p.sockets[0].serverClose(limit);
  await tick(10);
  assert.equal(p.sockets.length, 2, 'the camera opened a new session');
  assert.equal(p.toasts.length, 0, 'a quiet re-link');
  assert.equal(p.api.isCameraLive(), true);
  p.sockets[1].serverClose(limit); // immediately: not the real cap
  await tick(10);
  assert.equal(p.sockets.length, 2, 'no re-link loop');
  assert.equal(p.toasts.at(-1), limit.message);
  assert.equal(p.api.isCameraActive(), true);
  assert.equal(await p.api.speakText('Next line.'), true);
  assert.equal(p.sockets.length, 3, 'speakText reconnects instead of reusing the dead socket');
  assert.ok(p.sockets[2].sent.includes('speak'));
  p.api.stopCamera();
}

// 8. A session the server never answers is reported, not left hanging silently.
{
  const p = page({ status: VOICE_ON, media: camera, silent: true });
  await p.api.probe();
  const started = p.api.startMic({});
  await tick(5);
  p.sockets[0].readyState = 3;
  p.sockets[0].onclose?.({ code: 1006 }); // dropped before `connected`
  assert.equal(await started, false);
  assert.equal(p.api.lastFailure(), 'session', 'the page may dictate through the browser instead');
  assert.match(p.toasts.at(-1), /Could not open Suzy's live session/);
  assert.equal(live(p.acquired), 0);
}

console.log('live-voice: 8 scenarios passed');
process.exit(0); // the connect timeouts of closed fake sockets would keep the loop alive
