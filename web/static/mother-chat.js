/**
 * Mother chat panel v1 (S10-T3): a persistent, multi-turn chat with the Mother Agent.
 * POST /api/sessions/{sid}/chat streams the same field events as an intent — cards bloom
 * in the field while the synthesized reply lands in the transcript. GET on the same route
 * hydrates history after a refresh. Starter chips seed the first turn; `suggest` events
 * from the stream become tappable follow-ups. Live mode only; disable with
 * localStorage agentrix_p2=0.
 */
(function () {
  'use strict';

  const STARTERS = [
    "What's happening with work?",
    'Start my day',
    'Prepare me for my afternoon',
  ];

  async function boot() {
    if (window.__AGENTRIX_BOOT__) await window.__AGENTRIX_BOOT__;
    if (window.__AGENTRIX_DEMO__) return;
    if (localStorage.getItem('agentrix_p2') === '0') return;

    const openBtn = document.getElementById('chatOpen');
    const panel = document.getElementById('chatPanel');
    const list = document.getElementById('chatList');
    const form = document.getElementById('chatForm');
    const input = document.getElementById('chatText');
    const closeBtn = document.getElementById('chatClose');
    if (!openBtn || !panel || !list || !form || !input) return;

    let hydrated = false;
    let inFlight = false;
    let pendingEl = null;
    let streamedSuggestion = null;

    function turnEl(role, content, ts) {
      const d = document.createElement('div');
      d.className = 'chat-turn ' + role;
      if (role === 'user') d.textContent = content;
      else d.innerHTML = content; // Mother replies carry the same <b> markup as Suzy summaries
      if (ts) d.title = new Date(ts).toLocaleString();
      list.appendChild(d);
      list.scrollTop = list.scrollHeight;
      return d;
    }

    function chipRow(texts, hint) {
      const row = document.createElement('div');
      row.className = 'chat-chips';
      texts.forEach((t) => {
        const c = document.createElement('button');
        c.type = 'button';
        c.className = 'chat-chip';
        c.textContent = hint ? `${hint} ${t}` : t;
        c.addEventListener('click', () => send(t));
        row.appendChild(c);
      });
      list.appendChild(row);
      list.scrollTop = list.scrollHeight;
      return row;
    }

    function typingEl() {
      const d = turnEl('mother', '');
      d.classList.add('pending');
      d.innerHTML = '<span class="tdots"><i></i><i></i><i></i></span>';
      return d;
    }

    function setEmpty() {
      list.innerHTML = '<div class="p2-empty">Say hello — Suzy is listening.</div>';
      chipRow(STARTERS);
    }
    function clearEmpty() {
      const e = list.querySelector('.p2-empty');
      if (e) e.remove();
      list.querySelectorAll('.chat-chips').forEach((r) => r.remove());
    }

    async function hydrate() {
      if (hydrated) return;
      hydrated = true;
      const live = window.__AGENTRIX_LIVE__;
      try {
        await live?.ensureSession?.();
        const sid = live?.getSessionId?.();
        if (!sid) return setEmpty();
        const res = await fetch(`/api/sessions/${sid}/chat`, { credentials: 'include' });
        if (!res.ok) return setEmpty();
        const data = await res.json();
        const turns = data.turns || [];
        if (!turns.length) return setEmpty();
        turns.forEach((t) => turnEl(t.role === 'user' ? 'user' : 'mother', t.text, t.ts));
      } catch (_) {
        setEmpty();
      }
    }

    async function send(text) {
      const trimmed = (text || '').trim();
      if (!trimmed || inFlight) return;
      const live = window.__AGENTRIX_LIVE__;
      if (!live || !live.chat) {
        clearEmpty();
        turnEl('mother', 'Live orchestration is unavailable — check your connection.');
        return;
      }
      clearEmpty();
      input.value = '';
      streamedSuggestion = null;
      turnEl('user', trimmed, Date.now());
      pendingEl = typingEl();
      inFlight = true;
      try {
        await live.chat(trimmed);
      } catch (err) {
        if (err && err.name !== 'AbortError' && pendingEl) {
          pendingEl.classList.remove('pending');
          pendingEl.textContent = `Could not reach Suzy — ${err.message || 'try again'}.`;
          pendingEl = null;
        }
      } finally {
        inFlight = false;
        if (pendingEl && pendingEl.classList.contains('pending')) pendingEl.remove();
        pendingEl = null;
        if (streamedSuggestion) {
          chipRow([streamedSuggestion], 'Try:');
          streamedSuggestion = null;
        }
      }
    }

    function open() {
      document.getElementById('apprPanel')?.setAttribute('hidden', '');
      panel.removeAttribute('hidden');
      hydrate();
      input.focus();
    }
    function close() {
      panel.setAttribute('hidden', '');
    }
    openBtn.addEventListener('click', () => (panel.hidden ? open() : close()));
    if (closeBtn) closeBtn.addEventListener('click', close);
    input.addEventListener('keydown', (e) => {
      if (e.key === 'Escape') close();
    });

    form.addEventListener('submit', (e) => {
      e.preventDefault();
      send(input.value);
    });

    // The reply (and any follow-up suggestion) arrives on the shared event stream.
    window.addEventListener('agentrix:field-event', (e) => {
      if (!inFlight) return;
      const ev = e.detail || {};
      if (ev.type === 'suzy_summary') {
        if (pendingEl) {
          pendingEl.classList.remove('pending');
          pendingEl.innerHTML = ev.html || '';
          pendingEl.title = new Date().toLocaleString();
          pendingEl = null;
        } else {
          turnEl('mother', ev.html || '', Date.now());
        }
      } else if (ev.type === 'suggest' && ev.text) {
        streamedSuggestion = ev.text;
      } else if (ev.type === 'error' && pendingEl) {
        pendingEl.classList.remove('pending');
        pendingEl.textContent = ev.message || 'Something went wrong.';
        pendingEl = null;
      }
    });

    openBtn.removeAttribute('hidden');
  }

  boot().catch((err) => console.warn('[agentrix] mother-chat boot failed', err));
})();
