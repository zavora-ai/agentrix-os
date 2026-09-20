/**
 * Gemini Live bridge — WS /ws/voice (mia pattern).
 * One Live session carries two independent inputs: the microphone (PCM up, Suzy's speech back)
 * and the camera (JPEG frames up about once a second, gestures back as `agentrix:gesture`).
 * Either runs alone; the session opens with the first input and closes with the last.
 * Suzy's voice comes only from here: there are no prerecorded clips. When Live is unavailable
 * the page shows captions and the mic falls back to the browser's SpeechRecognition.
 */
(function () {
  'use strict';

  const INPUT_RATE = 16000;
  const OUTPUT_RATE = 24000;
  const FRAME_MS = 1000;
  const FRAME_W = 320;

  let enabled = false;
  let cameraEnabled = false;
  let ws = null;
  let connecting = null; // Promise<boolean> while the socket opens
  let sessionId = null;
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

  // Camera — frames are drawn to a small canvas and sent as JPEG; nothing is kept.
  let cameraActive = false;
  let camStream = null;
  let videoEl = null;
  let canvasEl = null;
  let frameTimer = null;
  let framesSent = 0;

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

  function handleMessage(ev) {
    if (typeof ev.data === 'string') {
      let msg;
      try {
        msg = JSON.parse(ev.data);
      } catch (_) {
        return;
      }
      if (msg.type === 'connected' && msg.session_id) {
        sessionId = msg.session_id;
        try {
          sessionStorage.setItem('agentrix_session_id', sessionId);
        } catch (_) {}
      }
      if (msg.type === 'connected' && typeof msg.camera === 'boolean') {
        cameraEnabled = enabled && msg.camera;
      }
      if (msg.type === 'connected' && msg.output_rate) {
        outputRate = msg.output_rate;
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
          const sock = ws;
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
          sessionStorage.setItem('agentrix_session_id', sid);
          sessionId = sid;
        }
        emit('agentrix:voice-intent', { sessionId: sid, args: msg.arguments });
      }
      if (msg.type === 'tool_call' && msg.name === 'ui_gesture' && msg.arguments?.gesture) {
        emit('agentrix:gesture', { gesture: msg.arguments.gesture });
      }
      if (msg.type === 'frame_rejected') {
        console.warn('live camera: frame rejected —', msg.reason);
        if (msg.reason === 'camera_off') stopCamera();
      }
      if (msg.type === 'error') {
        console.warn('live voice:', msg.message);
        flushPlayback();
        emit('agentrix:voice-error', { message: msg.message });
      }
      return;
    }
    if (ev.data instanceof ArrayBuffer) {
      playPcm(ev.data);
    } else if (ev.data instanceof Blob) {
      ev.data.arrayBuffer().then(playPcm);
    }
  }

  // ---- session -----------------------------------------------------------------------------

  function ensureSession(opts) {
    if (opts?.sessionId) sessionId = opts.sessionId;
    if (opts?.onTranscript) onTranscript = opts.onTranscript;
    if (!enabled) return Promise.resolve(false);
    if (sessionOpen()) return Promise.resolve(true);
    if (connecting) return connecting;
    connecting = new Promise((resolve) => {
      const sock = new WebSocket(wsUrl());
      sock.binaryType = 'arraybuffer';
      ws = sock;
      let settled = false;
      const settle = (ok) => {
        if (settled) return;
        settled = true;
        connecting = null;
        resolve(ok);
      };
      sock.onerror = () => {
        if (ws === sock) closeSession({ flush: true });
        settle(false);
      };
      sock.onclose = () => {
        if (ws === sock) {
          // The socket going away is not an interruption: whatever Suzy already sent keeps
          // playing to the end. Only barge-in and errors flush the queue.
          ws = null;
          stopMicCapture();
          stopCameraCapture();
        }
        settle(false);
      };
      sock.onmessage = handleMessage;
      sock.onopen = () => settle(true);
      setTimeout(() => {
        if (!settled) {
          try {
            sock.close();
          } catch (_) {}
          settle(false);
        }
      }, 8000);
    });
    return connecting;
  }

  function closeSession(opts) {
    const sock = ws;
    ws = null;
    if (sock) {
      try {
        sock.close();
      } catch (_) {}
    }
    stopMicCapture();
    stopCameraCapture();
    if (opts?.flush) flushPlayback();
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
    micStream = await navigator.mediaDevices.getUserMedia({
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

  /** Microphone on: the browser's permission prompt comes first (no deadline), then the session. */
  async function startMic(opts) {
    if (!enabled) return false;
    if (micActive) return true;
    try {
      await startMicCapture();
    } catch (e) {
      console.warn('live voice capture failed:', e);
      stopMicCapture();
      return false;
    }
    const ok = await ensureSession(opts);
    if (!ok) {
      stopMicCapture();
      return false;
    }
    micActive = true;
    emit('agentrix:mic', { active: true });
    return true;
  }

  function stopMic() {
    stopMicCapture();
    maybeCloseSession();
  }

  // ---- camera ------------------------------------------------------------------------------

  async function startCameraCapture() {
    camStream = await navigator.mediaDevices.getUserMedia({
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

  function sendFrame() {
    if (!cameraActive || !sessionOpen() || !videoEl) return;
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

  /** Camera on, with or without the microphone: permission prompt first, then the session. */
  async function startCamera(opts) {
    if (!cameraEnabled) return false;
    if (cameraActive) return true;
    try {
      await startCameraCapture();
    } catch (e) {
      console.warn('live camera capture failed:', e);
      stopCameraCapture();
      return false;
    }
    const ok = await ensureSession(opts);
    if (!ok) {
      stopCameraCapture();
      return false;
    }
    cameraActive = true;
    framesSent = 0;
    frameTimer = setInterval(sendFrame, FRAME_MS);
    emit('agentrix:camera', { active: true, stream: camStream });
    return true;
  }

  function stopCamera() {
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
    const ok = await ensureSession({});
    if (!ok || !ws) return false;
    ws.send(JSON.stringify({ type: 'speak', content: plain }));
    return true;
  }

  async function probe() {
    try {
      const res = await fetch('/api/voice/status');
      if (!res.ok) return false;
      const data = await res.json();
      enabled = !!data.enabled;
      cameraEnabled = enabled && !!data.camera;
      return enabled;
    } catch (_) {
      enabled = false;
      cameraEnabled = false;
      return false;
    }
  }

  window.AgentrixLiveVoice = {
    probe,
    // microphone
    startMic,
    stopMic,
    isMicActive: () => micActive,
    // camera
    startCamera,
    stopCamera,
    isCameraEnabled: () => cameraEnabled,
    isCameraActive: () => cameraActive,
    // session
    speakText,
    stopSpeaking,
    queuedSeconds,
    isEnabled: () => enabled,
    isActive: () => sessionOpen(),
    // legacy names (mic)
    start: startMic,
    stop: stopMic,
  };

  probe();
})();
