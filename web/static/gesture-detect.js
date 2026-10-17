/**
 * Local hand-gesture detection for the camera channel (M10-T5).
 *
 * Gemini Live receives the frames while a session is open, but a model only acts at turn
 * boundaries, so a silent user waving at the camera got no reaction. This runs on the camera
 * stream whether or not there is a Live session (no key, rejected key, session dropped): it
 * starts and stops only with the camera itself. It runs MediaPipe's hand landmarker + gesture
 * recogniser in the browser on the same camera stream (about 10 fps, nothing leaves the
 * machine), feeds the per-frame facts to the pure classifier in gesture-classifier.js, and
 * emits the same `agentrix:gesture` event the model path emits — so gestures.js, the ding and
 * the ledger event work identically for both. The library loads on first camera use from
 * jsDelivr; if it cannot load, the model path stays the only detector.
 */
(function () {
  'use strict';

  const CDN = 'https://cdn.jsdelivr.net/npm/@mediapipe/tasks-vision@0.10.14';
  const MODEL =
    'https://storage.googleapis.com/mediapipe-models/gesture_recognizer/gesture_recognizer/float16/1/gesture_recognizer.task';
  const TICK_MS = 90;

  let recognizer = null;
  let loading = null;
  let video = null;
  let stream = null;
  let timer = null;
  let classifier = null;
  let handSeen = false;
  // Bumped by every stop: a start still waiting for the model after a restart must not add a
  // second interval (the first one used to leak and run forever).
  let generation = 0;

  function emit(name, detail) {
    window.dispatchEvent(new CustomEvent(name, { detail }));
  }

  async function load() {
    if (recognizer) return recognizer;
    if (loading) return loading;
    loading = (async () => {
      emit('agentrix:gesture-detector', { state: 'loading' });
      const vision = await import(`${CDN}/vision_bundle.mjs`);
      const files = await vision.FilesetResolver.forVisionTasks(`${CDN}/wasm`);
      const make = (delegate) =>
        vision.GestureRecognizer.createFromOptions(files, {
          baseOptions: { modelAssetPath: MODEL, delegate },
          runningMode: 'VIDEO',
          numHands: 1,
          minHandDetectionConfidence: 0.5,
          minHandPresenceConfidence: 0.5,
          minTrackingConfidence: 0.5,
        });
      try {
        recognizer = await make('GPU');
      } catch (_) {
        recognizer = await make('CPU');
      }
      emit('agentrix:gesture-detector', { state: 'ready' });
      return recognizer;
    })().catch((err) => {
      loading = null;
      emit('agentrix:gesture-detector', { state: 'unavailable', reason: String(err).slice(0, 120) });
      throw err;
    });
    return loading;
  }

  function frameFacts(result, t) {
    const lm = result.landmarks && result.landmarks[0];
    if (!lm) return { t, present: false, category: 'None', palmX: 0.5, pinchDist: 1 };
    const cat = (result.gestures && result.gestures[0] && result.gestures[0][0]?.categoryName) || 'None';
    const palm = lm[9] || lm[0]; // middle-finger MCP: steadier than the wrist
    const dx = lm[4].x - lm[8].x;
    const dy = lm[4].y - lm[8].y;
    return { t, present: true, category: cat, palmX: palm.x, pinchDist: Math.hypot(dx, dy) };
  }

  function tick() {
    if (!recognizer || !video || video.readyState < 2) return;
    let result;
    try {
      result = recognizer.recognizeForVideo(video, performance.now());
    } catch (_) {
      return;
    }
    const facts = frameFacts(result, performance.now());
    if (facts.present !== handSeen) {
      handSeen = facts.present;
      emit('agentrix:gesture-detector', { state: 'ready', hand: handSeen, category: facts.category });
    }
    const gesture = classifier.push(facts);
    if (gesture) emit('agentrix:gesture', { gesture, source: 'local' });
  }

  async function start(source) {
    if (source === stream && video) return; // already running on this stream
    stop();
    const gen = generation;
    const api = window.AgentrixGestureClassifier;
    if (!api) return;
    stream = source;
    classifier = api.createGestureClassifier({ mirrored: false });
    video = document.createElement('video');
    video.muted = true;
    video.playsInline = true;
    video.srcObject = source;
    try {
      await video.play();
    } catch (_) {
      /* autoplay policy: the camera click that started this counts as the gesture */
    }
    try {
      await load();
    } catch (err) {
      console.warn('[agentrix] local gesture detection unavailable:', err);
      return;
    }
    if (gen !== generation || !video) return; // stopped or restarted while loading
    timer = setInterval(tick, TICK_MS);
  }

  function stop() {
    generation++;
    stream = null;
    if (timer) {
      clearInterval(timer);
      timer = null;
    }
    if (video) {
      try {
        video.pause();
        video.srcObject = null;
      } catch (_) {}
      video = null;
    }
    handSeen = false;
    classifier = null;
  }

  window.addEventListener('agentrix:camera', (e) => {
    if (e.detail?.active && e.detail.stream) start(e.detail.stream);
    else stop();
  });

  window.__AGENTRIX_GESTURE_DETECT__ = { isReady: () => !!recognizer, isRunning: () => !!timer };
})();
