/**
 * Gemini Live voice bridge — WS /ws/voice (mia pattern).
 * Falls back to prerecorded clips + SpeechRecognition when unavailable.
 * Camera channel (M10-T5): with voice active, still frames go up the same socket about once a
 * second and Suzy's `ui_gesture` tool call comes back as a `agentrix:gesture` event (gestures.js).
 */
(function () {
  'use strict';

  const INPUT_RATE = 16000;
  const OUTPUT_RATE = 24000;

  let enabled = false;
  let active = false;
  let ws = null;
  let mediaStream = null;
  let captureCtx = null;
  let playbackCtx = null;
  let processor = null;
  let sessionId = null;
  let onTranscript = null;

  // Camera channel — frames are drawn to a small canvas and sent as JPEG; nothing is kept.
  const FRAME_MS = 1000;
  const FRAME_W = 320;
  let cameraEnabled = false;
  let cameraActive = false;
  let camStream = null;
  let videoEl = null;
  let canvasEl = null;
  let frameTimer = null;

  function wsUrl() {
    const proto = location.protocol === 'https:' ? 'wss:' : 'ws:';
    const q = sessionId ? `?session_id=${encodeURIComponent(sessionId)}` : '';
    return `${proto}//${location.host}/ws/voice${q}`;
  }

  function playPcm(buffer) {
    playbackCtx = playbackCtx || new AudioContext({ sampleRate: OUTPUT_RATE });
    if (playbackCtx.state === 'suspended') playbackCtx.resume();
    const pcm16 = new Int16Array(buffer);
    const float32 = new Float32Array(pcm16.length);
    for (let i = 0; i < pcm16.length; i++) float32[i] = pcm16[i] / 32768;
    const audioBuffer = playbackCtx.createBuffer(1, float32.length, OUTPUT_RATE);
    audioBuffer.getChannelData(0).set(float32);
    const source = playbackCtx.createBufferSource();
    source.buffer = audioBuffer;
    source.connect(playbackCtx.destination);
    source.start();
    if (typeof window.drive === 'function' && typeof window.voiceTarget === 'function') {
      try {
        const el = document.createElement('audio');
        window.drive(el);
      } catch (_) {}
    }
  }

  async function startCapture() {
    mediaStream = await navigator.mediaDevices.getUserMedia({
      audio: {
        sampleRate: INPUT_RATE,
        channelCount: 1,
        echoCancellation: true,
        noiseSuppression: true,
      },
    });
    captureCtx = new AudioContext({ sampleRate: INPUT_RATE });
    const source = captureCtx.createMediaStreamSource(mediaStream);
    processor = captureCtx.createScriptProcessor(4096, 1, 1);
    processor.onaudioprocess = (e) => {
      if (!active || !ws || ws.readyState !== WebSocket.OPEN) return;
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

  function stopCapture() {
    if (processor) {
      processor.disconnect();
      processor = null;
    }
    if (captureCtx) {
      captureCtx.close().catch(() => {});
      captureCtx = null;
    }
    if (mediaStream) {
      mediaStream.getTracks().forEach((t) => t.stop());
      mediaStream = null;
    }
  }

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
      if (msg.type === 'tool_call' && msg.name === 'ui_gesture' && msg.arguments?.gesture) {
        window.dispatchEvent(new CustomEvent('agentrix:gesture', { detail: { gesture: msg.arguments.gesture } }));
      }
      if (msg.type === 'frame_rejected') {
        console.warn('live camera: frame rejected —', msg.reason);
        if (msg.reason === 'camera_off') stopCamera();
      }
      if (msg.type === 'transcript' && msg.content && onTranscript) {
        onTranscript(msg.content);
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
        window.dispatchEvent(
          new CustomEvent('agentrix:voice-intent', { detail: { sessionId: sid, args: msg.arguments } })
        );
      }
      if (msg.type === 'error') {
        console.warn('live voice:', msg.message);
      }
      return;
    }
    if (ev.data instanceof ArrayBuffer) {
      playPcm(ev.data);
    } else if (ev.data instanceof Blob) {
      ev.data.arrayBuffer().then(playPcm);
    }
  }

  async function start(opts) {
    if (!enabled || active) return false;
    sessionId = opts?.sessionId || null;
    onTranscript = opts?.onTranscript || null;

    return new Promise((resolve) => {
      ws = new WebSocket(wsUrl());
      ws.binaryType = 'arraybuffer';

      const fail = () => {
        stop();
        resolve(false);
      };

      ws.onerror = fail;
      ws.onclose = () => {
        if (active) stop();
      };
      ws.onmessage = handleMessage;
      ws.onopen = async () => {
        try {
          await startCapture();
          active = true;
          resolve(true);
        } catch (e) {
          console.warn('live voice capture failed:', e);
          fail();
        }
      };

      setTimeout(() => {
        if (!active) fail();
      }, 8000);
    });
  }

  async function startCamera() {
    if (!cameraEnabled || cameraActive || !active || !ws || ws.readyState !== WebSocket.OPEN) return false;
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
    cameraActive = true;
    frameTimer = setInterval(sendFrame, FRAME_MS);
    return true;
  }

  function sendFrame() {
    if (!cameraActive || !active || !ws || ws.readyState !== WebSocket.OPEN) return;
    const vw = videoEl.videoWidth;
    const vh = videoEl.videoHeight;
    if (!vw || !vh) return;
    canvasEl.width = FRAME_W;
    canvasEl.height = Math.max(1, Math.round((FRAME_W * vh) / vw));
    canvasEl.getContext('2d').drawImage(videoEl, 0, 0, canvasEl.width, canvasEl.height);
    const url = canvasEl.toDataURL('image/jpeg', 0.6);
    const data = url.slice(url.indexOf(',') + 1);
    if (data) ws.send(JSON.stringify({ type: 'frame', mime: 'image/jpeg', data }));
  }

  function stopCamera() {
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
  }

  function stop() {
    active = false;
    stopCamera();
    stopCapture();
    if (ws) {
      try {
        ws.close();
      } catch (_) {}
      ws = null;
    }
  }

  async function speakText(text) {
    if (!enabled) return false;
    const ok = await start({});
    if (!ok || !ws) return false;
    ws.send(JSON.stringify({ type: 'text', content: text }));
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
    start,
    stop,
    speakText,
    isEnabled: () => enabled,
    isActive: () => active,
    startCamera,
    stopCamera,
    isCameraEnabled: () => cameraEnabled,
    isCameraActive: () => cameraActive,
  };

  probe();
})();