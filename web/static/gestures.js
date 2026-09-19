/**
 * Camera gestures → UI verbs (M10-T5).
 * live-voice.js relays Suzy's `ui_gesture` tool call as a `agentrix:gesture` event; this maps it
 * to the same actions a click or key would take, through the normal routes, so the permission
 * gate and audit apply unchanged. Live mode only — the demo tour never sees a camera.
 */
(function () {
  'use strict';

  const BRIEFING = 'What do I need to know today?';

  function toast(msg) {
    window.__AGENTRIX_UI__?.showSuzyCustom?.(msg);
  }

  function sessionId() {
    return window.__AGENTRIX_LIVE__?.getSessionId?.() || sessionStorage.getItem('agentrix_session_id') || null;
  }

  async function pauseAgents() {
    try {
      const res = await fetch('/api/pause', {
        method: 'POST',
        credentials: 'include',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ session_id: sessionId() }),
      });
      if (res.status === 401 || res.status === 403) {
        toast('Sign in to pause the agents.');
        return;
      }
      if (!res.ok) {
        toast('Could not pause the agents.');
        return;
      }
      toast('Agents paused — nothing runs until you resume.');
    } catch (_) {
      toast('Could not reach the server.');
    }
  }

  function onGesture(ev) {
    if (window.__AGENTRIX_DEMO__) return;
    const gesture = ev.detail?.gesture;
    const lens = window.__AGENTRIX_LENS__;
    switch (gesture) {
      case 'swipe_left':
        lens?.step?.(1); // toward Home, like a touch swipe
        break;
      case 'swipe_right':
        lens?.step?.(-1); // toward Work
        break;
      case 'open_palm':
        pauseAgents();
        break;
      case 'wave':
        window.dispatchEvent(
          new CustomEvent('agentrix:voice-intent', { detail: { sessionId: sessionId(), args: { text: BRIEFING } } })
        );
        break;
      default:
        return;
    }
    const world = document.body.dataset.world;
    window.__AGENTRIX_LIVE__?.recordUiEvent?.('ui_gesture', {
      domain: world === 'work' || world === 'home' ? world : 'shared',
    });
  }

  window.addEventListener('agentrix:gesture', onGesture);
})();
