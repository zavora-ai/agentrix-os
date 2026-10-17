/**
 * Gemini Live bridge — WS /ws/voice (mia pattern).
 * One Live session carries two independent inputs: the microphone (PCM up, Suzy's speech back)
 * and the camera (JPEG frames up about once a second, gestures back as `agentrix:gesture`).
 * Either runs alone; the session opens with the first input and closes with the last.
 * Suzy's voice comes only from here: there are no prerecorded clips. When Live is unavailable
 * the page shows captions and the mic falls back to the browser's SpeechRecognition.
 * The camera does not need Live: local gesture detection (gesture-detect.js) runs on its stream
 * whatever happens to the session, and frames go to Gemini only while a session is open.
 * Failures are shown in Suzy's toast, never only logged: a denied or missing device, and a
 * session the server refused or closed (it sends `{type: "error"|"closed", reason, message}`).
 */
(function () {
  'use strict';

  const INPUT_RATE = 16000;
  const OUTPUT_RATE = 24000;
  const FRAME_MS = 1000;
  const FRAME_W = 320;
  const CONNECT_MS = 10000;
  // A session that reached Gemini's time limit (documented: about ten minutes per connection, or
  // two with video when the server does not ask for context window compression) is re-linked for
  // the camera only if it lasted this long — a session "ending" sooner is not the cap, and
  // re-linking it would loop.
  const RELINK_MIN_MS = 20000;
  const SESSION_KEY = 'agentrix_session_id';

  let enabled = false; // a Live session can open (the server has a key Gemini has not rejected)
  let unavailableReason = null;
  let liveCamera = false; // frames may go to Gemini (camera channel on the Live session)
  let cameraEnabled = false; // the 📷 button: local gestures, with or without Live
  let ws = null;
  let connecting = null; // Promise<boolean> until the server says `connected` (or refuses)
  let sessionId = null;
  let sessionStartedAt = 0; // when the current session said `connected`
  let lastFailure = null; // why the last start returned false: device, session or cancelled
  let onTranscript = null; // receives the USER's words (input transcription), for the intent bar
  let playbackCtx = null;
  let outputRate = OUTPUT_RATE; // negotiated by the server's `connected` message
  // Gapless playback (mia pattern): chunks arrive faster than real time, so they are queued
  // back to back at `nextPlayTime`; `liveSources` lets barge-in stop what has not played yet.
  let nextPlayTime = 0;
  let liveSources = [];
  let userUtterance = ''; // coalesced input-transcript deltas for the current user turn

  // Microphone
  let micActive = false;
  let micStream = null;
  let captureCtx = null;
  let processor = null;
  // In-flight guards: a second click while the permission prompt or the session was pending
  // started a second capture and leaked the first stream (the camera light stayed on).
  let micStarting = null;
  let micCancel = false;

  // Camera — frames are drawn to a small canvas and sent as JPEG; nothing is kept.
  let cameraActive = false;
  let camStream = null;
  let videoEl = null;
  let canvasEl = null;
  let frameTimer = null;
  let framesSent = 0;
  let camStarting = null;
  let camCancel = false;

  function wsUrl() {
    const proto = location.protocol === 'https:' ? 'wss:' : 'ws:';
    const q = sessionId ? `?session_id=${encodeURIComponent(sessionId)}` : '';
    return `${proto}//${location.host}/ws/voice${q}`;
  }

  function sessionOpen() {
    return !!ws && ws.readyState === WebSocket.OPEN;
  }

  function emit(name, detail) {
    window.dispatchEvent(new CustomEvent(name, { detail }));
  }

  function escapeHtml(text) {
    return String(text).replace(
      /[&<>"']/g,
      (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]
    );
  }

  // Suzy's toast (the one the field uses); the same notice twice in a row is shown once.
  let lastNotice = '';
  let lastNoticeAt = 0;
  function notify(text) {
    const now = Date.now();
    if (text === lastNotice && now - lastNoticeAt < 4000) return;
    lastNotice = text;
    lastNoticeAt = now;
    console.warn('[agentrix] live:', text);
    if (window.__AGENTRIX_DEMO__) return; // the scripted tour owns the toast
    window.__AGENTRIX_UI__?.showSuzyCustom?.(escapeHtml(text));
  }

  /** What to tell the user when getUserMedia fails. */
  function deviceMessage(kind, err) {
    const Kind = kind[0].toUpperCase() + kind.slice(1);
    switch (err?.name) {
      case 'NotAllowedError':
      case 'SecurityError':
        return `${Kind} permission denied — allow it for this site (the icon in the address bar), then try again.`;
      case 'NotFoundError':
      case 'OverconstrainedError':
        return `No ${kind} found — connect one and try again.`;
      case 'NotReadableError':
      case 'AbortError':
        return `The ${kind} is busy — another app may be using it.`;
      case 'InsecureContextError':
        return `This page cannot use the ${kind}: browsers allow it only on https or localhost.`;
      default:
        return `Could not start the ${kind}${err?.message ? `: ${err.message}` : '.'}`;
    }
  }

  function getUserMedia(constraints) {
    if (!navigator.mediaDevices?.getUserMedia) {
      const err = new Error('getUserMedia unavailable');
      err.name = 'InsecureContextError';
      return Promise.reject(err);
    }
    return navigator.mediaDevices.getUserMedia(constraints);
  }

  /** Voice is off for this page (no key, or Gemini rejected it): the mic dictates through the
   *  browser and the camera runs local gestures only. */
  function setUnavailable(reason) {
    enabled = false;
    liveCamera = false;
    unavailableReason = reason || unavailableReason;
    emit('agentrix:voice-status', { enabled: false, reason: unavailableReason, camera: cameraEnabled });
  }

  function storedSessionId() {
    try {
      return sessionStorage.getItem(SESSION_KEY);
    } catch (_) {
      return null;
    }
  }

  /** The page's own session (field-client.js), so the voice socket joins it instead of the
   *  server minting one. Waits briefly for the page to create it at boot. */
  async function pageSessionId() {
    const live = window.__AGENTRIX_LIVE__;
    if (live?.whenSession) {
      try {
        await Promise.race([live.whenSession(), new Promise((r) => setTimeout(r, 3000))]);
      } catch (_) {}
    }
    return live?.getSessionId?.() || storedSessionId();
  }

  /** Schedule one PCM16 chunk for gapless playback (never start it "now" — that overlaps chunks). */
  function playPcm(buffer) {
    playbackCtx = playbackCtx || new AudioContext({ sampleRate: outputRate });
    if (playbackCtx.state === 'suspended') playbackCtx.resume();
    if (buffer.byteLength % 2) buffer = buffer.slice(0, buffer.byteLength - 1);
    const pcm16 = new Int16Array(buffer);
    if (!pcm16.length) return;
    const audioBuffer = playbackCtx.createBuffer(1, pcm16.length, outputRate);
    const ch = audioBuffer.getChannelData(0);
    for (let i = 0; i < pcm16.length; i++) ch[i] = pcm16[i] / 0x8000;
    const src = playbackCtx.createBufferSource();
    src.buffer = audioBuffer;
    src.connect(playbackCtx.destination);
    const now = playbackCtx.currentTime;
    if (nextPlayTime < now) nextPlayTime = now;
    src.start(nextPlayTime);
    nextPlayTime += audioBuffer.duration;
    liveSources.push(src);
    src.onended = () => {
      liveSources = liveSources.filter((s) => s !== src);
    };
  }

  /** Barge-in / interruption: drop everything Suzy has not said yet. */
  function flushPlayback() {
    liveSources.forEach((s) => {
      try {
        s.stop();
      } catch (_) {}
    });
    liveSources = [];
    nextPlayTime = 0;
  }

  /** Seconds of speech still queued. Gemini streams a reply faster than real time, so at
   *  `response_done` most of it is usually still waiting to be played. */
  function queuedSeconds() {
    if (!playbackCtx) return 0;
    return Math.max(0, nextPlayTime - playbackCtx.currentTime);
  }

  /** Run `fn` once everything queued has been heard (plus a short tail), not before. */
  function afterPlayback(fn) {
    setTimeout(fn, queuedSeconds() * 1000 + 250);
  }

  // ---- messages from the server ----------------------------------------------------------

  /** Returns the message type, so a socket that is still opening can settle on it. */
  function handleMessage(ev, sock) {
    if (typeof ev.data === 'string') {
      let msg;
      try {
        msg = JSON.parse(ev.data);
      } catch (_) {
        return null;
      }
      if (msg.type === 'connected') sessionStartedAt = Date.now();
      if (msg.type === 'connected' && msg.session_id) {
        sessionId = msg.session_id;
        // A page that already has a session keeps it; overwriting it here made every later
        // intent run in whatever session the voice socket happened to get.
        if (!window.__AGENTRIX_LIVE__?.getSessionId?.() && !storedSessionId()) {
          try {
            sessionStorage.setItem(SESSION_KEY, sessionId);
          } catch (_) {}
        }
      }
      if (msg.type === 'connected' && typeof msg.camera === 'boolean') {
        liveCamera = enabled && msg.camera;
      }
      if (msg.type === 'connected' && msg.output_rate) {
        outputRate = msg.output_rate;
      }
      if (msg.type === 'connected' && cameraActive && liveCamera) {
        emit('agentrix:camera-live', { live: true });
      }
      // The user started a new turn: the server has already cut the model off; drop the
      // audio we had queued so Suzy stops talking right away.
      if (msg.type === 'speech_started') {
        flushPlayback();
        userUtterance = '';
        emit('agentrix:voice-user-speaking', {});
      }
      // Suzy's words (output transcription) — for captions, never for the intent bar.
      if (msg.type === 'transcript' && msg.content) {
        emit('agentrix:voice-transcript', { role: 'assistant', content: msg.content });
      }
      // The user's words (input transcription): Gemini streams deltas, so keep the running
      // utterance in the intent bar; a completed transcript (OpenAI-style) replaces it.
      if (msg.type === 'user_transcript_delta' && msg.content) {
        userUtterance += msg.content;
        if (onTranscript) onTranscript(userUtterance);
        emit('agentrix:voice-user-transcript', { content: userUtterance, done: false });
      }
      if (msg.type === 'user_transcript') {
        const text = (msg.content || userUtterance).trim();
        if (text && onTranscript) onTranscript(text);
        emit('agentrix:voice-user-transcript', { content: text, done: true });
        userUtterance = '';
      }
      if (msg.type === 'response_done') {
        emit('agentrix:voice-transcript', { role: 'assistant', done: true });
        // A session opened only to speak (greeting, a summary) has nothing left to do once
        // the queued audio has actually been heard — closing on the event itself cut Suzy
        // off mid-sentence, because the chunks arrive well ahead of playback.
        if (!micActive && !cameraActive) {
          afterPlayback(() => {
            if (ws === sock && !micActive && !cameraActive) closeSession();
          });
        }
      }
      if (msg.type === 'tool_call') {
        // adk-realtime forwards tool arguments as the raw JSON string the model produced.
        if (typeof msg.arguments === 'string') {
          try {
            msg.arguments = JSON.parse(msg.arguments);
          } catch (_) {
            msg.arguments = {};
          }
        }
      }
      if (msg.type === 'tool_call' && msg.name === 'submit_intent') {
        const sid = msg.arguments?.session_id || sessionId;
        if (sid) {
          try {
            sessionStorage.setItem(SESSION_KEY, sid);
          } catch (_) {}
          sessionId = sid;
        }
        emit('agentrix:voice-intent', { sessionId: sid, args: msg.arguments });
      }
      if (msg.type === 'tool_call' && msg.name === 'ui_gesture' && msg.arguments?.gesture) {
        emit('agentrix:gesture', { gesture: msg.arguments.gesture });
      }
      if (msg.type === 'frame_rejected') {
        console.warn('live camera: frame rejected —', msg.reason);
        // This session has no camera channel; local gestures keep running.
        if (msg.reason === 'camera_off') {
          liveCamera = false;
          emit('agentrix:camera-live', { live: false });
        }
      }
      // The server refused the session (`error` with a reason) or Gemini ended it (`closed`):
      // say why, drop the socket so the next start or speakText reconnects, and turn voice off
      // for this page when the key itself is the problem. A reasonless `error` is Gemini
      // reporting a problem mid-session; the session goes on.
      if (msg.type === 'closed' || msg.type === 'error') {
        const ending = msg.type === 'closed' || !!msg.reason;
        if (msg.reason === 'key_rejected' || msg.reason === 'voice_unavailable') setUnavailable(msg.message);
        // The camera is a standing input: at the time limit it re-links quietly instead of
        // leaving Suzy blind until the user turns 📷 off and on. The mic stops with the session.
        const relink =
          msg.reason === 'session_limit' && ws === sock && cameraActive && liveCamera &&
          Date.now() - sessionStartedAt >= RELINK_MIN_MS;
        const micWas = micActive;
        if (ending) dropSocket(sock, { flush: msg.type === 'error' });
        else flushPlayback();
        if (relink) {
          if (micWas) notify("Suzy's live session reached its time limit — the camera reconnected; tap 🎤 to talk again.");
          ensureSession({}).then((ok) => {
            if (ok && !cameraActive) maybeCloseSession();
          });
        } else {
          notify(msg.message || "Suzy's voice hit a problem.");
        }
        emit('agentrix:voice-error', { message: msg.message, reason: msg.reason || null });
      }
      return msg.type;
    }
    if (ev.data instanceof ArrayBuffer) {
      playPcm(ev.data);
    } else if (ev.data instanceof Blob) {
      ev.data.arrayBuffer().then(playPcm);
    }
    return null;
  }

  // ---- session -----------------------------------------------------------------------------

  /** Open the Live session if needed. Resolves true once the server says `connected` (Gemini
   *  took the session), false when it refuses, drops or times out — the reason is shown. */
  function ensureSession(opts) {
    if (opts?.onTranscript) onTranscript = opts.onTranscript;
    if (!enabled) return Promise.resolve(false);
    if (sessionOpen()) return Promise.resolve(true);
    if (connecting) return connecting;
    connecting = (async () => {
      sessionId = opts?.sessionId || (await pageSessionId()) || sessionId;
      if (!enabled) return false;
      return openSocket();
    })().finally(() => {
      connecting = null;
    });
    return connecting;
  }

  function openSocket() {
    return new Promise((resolve) => {
      const sock = new WebSocket(wsUrl());
      sock.binaryType = 'arraybuffer';
      ws = sock;
      let settled = false;
      let connected = false;
      let errored = false;
      const settle = (ok) => {
        if (settled) return;
        settled = true;
        resolve(ok);
      };
      sock.onerror = () => {
        errored = true;
      };
      sock.onclose = () => {
        // Still ours: nobody closed it on purpose and the server sent no reason.
        if (ws === sock) {
          const inUse = micActive || cameraActive || queuedSeconds() > 0;
          // The socket going away is not an interruption: whatever Suzy already sent keeps
          // playing to the end. Only barge-in and errors flush the queue.
          dropSocket(sock, { flush: errored });
          if (!connected) notify("Could not open Suzy's live session — the voice server did not answer.");
          else if (inUse) notify("Suzy's live session dropped. Tap 🎤 or 📷 to reconnect.");
        }
        settle(false);
      };
      sock.onmessage = (ev) => {
        const type = handleMessage(ev, sock);
        if (type === 'connected') {
          connected = true;
          settle(true);
        } else if (ws !== sock) {
          settle(false); // refused or closed while opening
        }
      };
      setTimeout(() => {
        if (settled) return;
        dropSocket(sock);
        notify("Suzy's live session did not start in time — try again.");
        settle(false);
      }, CONNECT_MS);
    });
  }

  /** Forget `sock` (if it is still the session) and close it. The mic needs the session and
   *  stops with it; the camera does not: local gestures keep running on its stream. */
  function dropSocket(sock, opts) {
    if (!sock || ws !== sock) return;
    ws = null;
    try {
      sock.close();
    } catch (_) {}
    stopMicCapture();
    if (opts?.flush) flushPlayback();
    if (cameraActive) emit('agentrix:camera-live', { live: false });
  }

  function closeSession(opts) {
    dropSocket(ws, opts);
  }

  /** Cut Suzy off (the user moved on): drop queued speech; a speak-only session ends here. */
  function stopSpeaking() {
    flushPlayback();
    if (!micActive && !cameraActive) closeSession();
  }

  function maybeCloseSession() {
    if (!micActive && !cameraActive) closeSession();
  }

  // ---- microphone --------------------------------------------------------------------------

  async function startMicCapture() {
    micStream = await getUserMedia({
      audio: { sampleRate: INPUT_RATE, channelCount: 1, echoCancellation: true, noiseSuppression: true },
    });
    captureCtx = new AudioContext({ sampleRate: INPUT_RATE });
    const source = captureCtx.createMediaStreamSource(micStream);
    // 2048 frames ≈ 128 ms at 16 kHz: the mia example's size, half the latency of 4096.
    processor = captureCtx.createScriptProcessor(2048, 1, 1);
    processor.onaudioprocess = (e) => {
      if (!micActive || !sessionOpen()) return;
      const input = e.inputBuffer.getChannelData(0);
      const pcm16 = new Int16Array(input.length);
      for (let i = 0; i < input.length; i++) {
        const s = Math.max(-1, Math.min(1, input[i]));
        pcm16[i] = s < 0 ? s * 0x8000 : s * 0x7fff;
      }
      ws.send(pcm16.buffer);
    };
    source.connect(processor);
    processor.connect(captureCtx.destination);
  }

  function stopMicCapture() {
    const was = micActive;
    micActive = false;
    if (processor) {
      processor.disconnect();
      processor = null;
    }
    if (captureCtx) {
      captureCtx.close().catch(() => {});
      captureCtx = null;
    }
    if (micStream) {
      micStream.getTracks().forEach((t) => t.stop());
      micStream = null;
    }
    if (was) emit('agentrix:mic', { active: false });
  }

  /** Microphone on: the browser's permission prompt comes first (no deadline), then the session.
   *  On false, `lastFailure()` says why: the device (already explained in a toast), the session
   *  (the page may dictate through the browser instead) or a stop while starting. */
  function startMic(opts) {
    if (!enabled) {
      lastFailure = 'session';
      return Promise.resolve(false);
    }
    if (micActive) return Promise.resolve(true);
    if (micStarting) return micStarting;
    micCancel = false;
    lastFailure = null;
    micStarting = (async () => {
      try {
        await startMicCapture();
      } catch (e) {
        stopMicCapture();
        if (micCancel) {
          lastFailure = 'cancelled';
        } else {
          lastFailure = 'device';
          notify(deviceMessage('microphone', e));
        }
        return false;
      }
      const ok = !micCancel && (await ensureSession(opts));
      if (!ok || micCancel) {
        stopMicCapture();
        lastFailure = micCancel ? 'cancelled' : 'session';
        if (ok) maybeCloseSession(); // stopped while it was opening: nobody needs it now
        return false;
      }
      micActive = true;
      emit('agentrix:mic', { active: true });
      return true;
    })().finally(() => {
      micStarting = null;
    });
    return micStarting;
  }

  function stopMic() {
    if (micStarting) micCancel = true;
    stopMicCapture();
    maybeCloseSession();
  }

  // ---- camera ------------------------------------------------------------------------------

  async function startCameraCapture() {
    camStream = await getUserMedia({
      video: { width: { ideal: 640 }, height: { ideal: 360 }, frameRate: { ideal: 5, max: 10 }, facingMode: 'user' },
      audio: false,
    });
    videoEl = document.createElement('video');
    videoEl.muted = true;
    videoEl.playsInline = true;
    videoEl.srcObject = camStream;
    await videoEl.play();
    canvasEl = document.createElement('canvas');
  }

  /** One frame to Gemini — only while a Live session with a camera channel is open. */
  function sendFrame() {
    if (!cameraActive || !liveCamera || !sessionOpen() || !videoEl) return;
    const vw = videoEl.videoWidth;
    const vh = videoEl.videoHeight;
    if (!vw || !vh) return;
    canvasEl.width = FRAME_W;
    canvasEl.height = Math.max(1, Math.round((FRAME_W * vh) / vw));
    canvasEl.getContext('2d').drawImage(videoEl, 0, 0, canvasEl.width, canvasEl.height);
    const url = canvasEl.toDataURL('image/jpeg', 0.6);
    const data = url.slice(url.indexOf(',') + 1);
    if (data) {
      ws.send(JSON.stringify({ type: 'frame', mime: 'image/jpeg', data }));
      framesSent++;
      emit('agentrix:camera-frame', { count: framesSent });
    }
  }

  function stopCameraCapture() {
    const was = cameraActive;
    cameraActive = false;
    if (frameTimer) {
      clearInterval(frameTimer);
      frameTimer = null;
    }
    if (videoEl) {
      try {
        videoEl.pause();
        videoEl.srcObject = null;
      } catch (_) {}
      videoEl = null;
    }
    canvasEl = null;
    if (camStream) {
      camStream.getTracks().forEach((t) => t.stop());
      camStream = null;
    }
    if (was) emit('agentrix:camera', { active: false });
  }

  /** Camera on, with or without the microphone and with or without Live: permission prompt
   *  first, then local gestures start on the stream (`agentrix:camera`), then — when voice is
   *  available — the Live session; frames follow while it stays open. */
  function startCamera(opts) {
    if (!cameraEnabled) return Promise.resolve(false);
    if (cameraActive) return Promise.resolve(true);
    if (camStarting) return camStarting;
    camCancel = false;
    lastFailure = null;
    camStarting = (async () => {
      try {
        await startCameraCapture();
      } catch (e) {
        stopCameraCapture();
        if (camCancel) {
          lastFailure = 'cancelled';
        } else {
          lastFailure = 'device';
          notify(deviceMessage('camera', e));
        }
        return false;
      }
      if (camCancel) {
        stopCameraCapture();
        lastFailure = 'cancelled';
        return false;
      }
      cameraActive = true;
      framesSent = 0;
      frameTimer = setInterval(sendFrame, FRAME_MS);
      emit('agentrix:camera', { active: true, stream: camStream, live: false });
      if (liveCamera) {
        // Not awaited: the camera is useful already; a refused session explains itself.
        // (`connected` announces `agentrix:camera-live`.)
        ensureSession(opts).then((ok) => {
          if (ok && !cameraActive) maybeCloseSession(); // turned off while the session was opening
        });
      }
      return true;
    })().finally(() => {
      camStarting = null;
    });
    return camStarting;
  }

  function stopCamera() {
    if (camStarting) camCancel = true;
    stopCameraCapture();
    maybeCloseSession();
  }

  // ---- speech only (greeting) --------------------------------------------------------------

  /** Have Suzy say `text` aloud (greeting, a summary). The server frames it as a read-aloud
   *  request so the model speaks the words instead of answering them as a user turn. */
  async function speakText(text) {
    if (!enabled) return false;
    const plain = String(text || '').trim();
    if (!plain) return false;
    // A session Gemini already ended was dropped when its `closed` arrived, so this opens a
    // new one instead of writing into a dead socket.
    const ok = await ensureSession({});
    if (!ok || !sessionOpen()) return false;
    ws.send(JSON.stringify({ type: 'speak', content: plain }));
    return true;
  }

  async function probe() {
    try {
      const res = await fetch('/api/voice/status');
      if (!res.ok) throw new Error(`voice status ${res.status}`);
      const data = await res.json();
      enabled = !!data.enabled;
      unavailableReason = enabled ? null : data.reason || null;
      liveCamera = enabled && !!data.camera;
      // `gestures`: the camera works without voice (local detection). Older servers lack it.
      cameraEnabled = typeof data.gestures === 'boolean' ? data.gestures : liveCamera;
    } catch (_) {
      enabled = false;
      liveCamera = false;
      cameraEnabled = false;
    }
    emit('agentrix:voice-status', { enabled, reason: unavailableReason, camera: cameraEnabled });
    return enabled;
  }

  window.AgentrixLiveVoice = {
    probe,
    // microphone
    startMic,
    stopMic,
    isMicActive: () => micActive,
    isMicStarting: () => !!micStarting,
    // camera
    startCamera,
    stopCamera,
    isCameraEnabled: () => cameraEnabled,
    isCameraActive: () => cameraActive,
    isCameraStarting: () => !!camStarting,
    isCameraLive: () => cameraActive && liveCamera && sessionOpen(),
    // session
    speakText,
    stopSpeaking,
    queuedSeconds,
    isEnabled: () => enabled,
    unavailableReason: () => unavailableReason,
    lastFailure: () => lastFailure,
    isActive: () => sessionOpen(),
    // legacy names (mic)
    start: startMic,
    stop: stopMic,
  };

  probe();
})();
