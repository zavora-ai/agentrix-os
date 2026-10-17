/**
 * Camera window (M10-T5): shows what the camera sends and what comes back — the live feed,
 * frames sent, the last gesture Suzy recognised and what it did, and her spoken reply.
 * Listens to the events live-voice.js and gestures.js emit; never touches the frames.
 * Without a Live session (voice off, key rejected, session dropped) the camera still runs
 * local gesture detection, and the caption says so instead of claiming Suzy is watching.
 */
(function () {
  'use strict';

  const EFFECT = {
    swipe_left: '👉 swipe → Home world',
    swipe_right: '👈 swipe → Work world',
    open_palm: '✋ open palm → agents paused',
    wave: "👋 wave → today's briefing",
    pinch: '🤏 pinch → window closed',
  };

  let box = null, video = null, status = null, last = null;
  let reply = '', replyDone = false;
  let live = false, detail = '';

  function render() {
    if (!status) return;
    const who = live ? 'Suzy is watching' : 'Gestures on this device';
    status.textContent = detail ? `${who} · ${detail}` : who;
  }

  function ensure() {
    if (box) return box;
    box = document.createElement('aside');
    box.className = 'cam-preview';
    box.setAttribute('aria-label', 'Camera — what Suzy sees');
    box.innerHTML =
      '<video autoplay muted playsinline></video>' +
      '<div class="cam-cap"><span class="cam-dot"></span><span class="cam-status">Suzy is watching</span></div>' +
      '<div class="cam-last">Try a wave, an open palm, or a swipe</div>';
    document.body.appendChild(box);
    video = box.querySelector('video');
    status = box.querySelector('.cam-status');
    last = box.querySelector('.cam-last');
    return box;
  }

  window.addEventListener('agentrix:camera', (e) => {
    if (e.detail?.active) {
      ensure();
      video.srcObject = e.detail.stream;
      live = !!e.detail.live;
      detail = '';
      render();
      box.classList.add('on');
    } else if (box) {
      box.classList.remove('on');
      video.srcObject = null;
    }
  });

  // The Live session came up or went away under a running camera.
  window.addEventListener('agentrix:camera-live', (e) => {
    live = !!e.detail?.live;
    if (!live) detail = ''; // no stale frame count under a local-only caption
    render();
  });

  window.addEventListener('agentrix:camera-frame', (e) => {
    const n = e.detail?.count || 0;
    live = true;
    detail = `${n} frame${n === 1 ? '' : 's'} sent`;
    render();
  });

  window.addEventListener('agentrix:gesture', (e) => {
    if (!last) return;
    const g = e.detail?.gesture;
    last.textContent = EFFECT[g] || `gesture: ${g}`;
    last.classList.add('hit');
    setTimeout(() => last.classList.remove('hit'), 1800);
  });

  // Local detector state (gesture-detect.js): loading, ready, hand in view, or unavailable.
  window.addEventListener('agentrix:gesture-detector', (e) => {
    const d = e.detail || {};
    if (d.state === 'loading') detail = 'loading gesture detector…';
    else if (d.state === 'unavailable') detail = 'local gestures unavailable';
    else if (d.state === 'ready') {
      const cat = d.category && d.category !== 'None' ? ` (${d.category.replace('_', ' ').toLowerCase()})` : '';
      detail = d.hand ? `hand in view${cat}` : 'gestures ready';
    }
    render();
  });

  window.addEventListener('agentrix:voice-transcript', (e) => {
    if (!last) return;
    if (e.detail?.done) { replyDone = true; return; }
    if (replyDone) { reply = ''; replyDone = false; }
    reply += e.detail?.content || '';
    const shown = reply.trim();
    if (shown) last.textContent = 'Suzy: ' + (shown.length > 110 ? '…' + shown.slice(-110) : shown);
  });
})();
