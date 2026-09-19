/**
 * Live mode: consume POST /api/sessions/{id}/intent|action SSE and drive the field UI.
 * Offline demo uses legacy scenarios{} when ?demo=1 or localStorage agentrix_demo=1.
 */
(function () {
  'use strict';

  async function boot() {
    if (window.__AGENTRIX_BOOT__) await window.__AGENTRIX_BOOT__;

    const ui = window.__AGENTRIX_UI__;
    if (!ui || window.__AGENTRIX_DEMO__) return;

    let sessionId = null;
    let abortController = null;
    const cards = new Map();

    const ACTION_RE =
      /\b(do it|combine|merge|fold|book|hold|reply|draft|reserve|apply|arrange|organi[sz]e|sort it|handle it|take care|read aloud|show me|reply to them)\b/i;

    const SESSION_KEY = 'agentrix_session_id';

    function rememberSession(id) {
      sessionId = id;
      try {
        sessionStorage.setItem(SESSION_KEY, id);
      } catch (_) {
        /* private browsing */
      }
    }

    async function ensureSession() {
      if (sessionId) return sessionId;
      const res = await fetch('/api/sessions', { method: 'POST', credentials: 'include' });
      if (!res.ok) throw new Error('session create failed');
      const data = await res.json();
      rememberSession(data.session_id);
      return sessionId;
    }

    async function hydrateFromServer(sid) {
      const [cardsRes, agentsRes] = await Promise.all([
        fetch(`/api/sessions/${sid}/cards`, { credentials: 'include' }),
        fetch(`/api/sessions/${sid}/agents`, { credentials: 'include' }),
      ]);
      if (cardsRes.status === 403) return false;
      if (!cardsRes.ok) return false;
      const cardsData = await cardsRes.json();
      if (!cardsData.cards?.length) return false;
      rememberSession(sid);
      if (ui.hydrateField) ui.hydrateField(cardsData);
      if (agentsRes.ok && ui.hydrateAgents) {
        ui.hydrateAgents(await agentsRes.json());
      }
      cardsData.cards.forEach((entry, i) => {
        const spec = entry.card || entry;
        if (entry.domain && !spec.domain) spec.domain = entry.domain;
        cards.set(entry.index ?? i, { spec, card: { el: null } });
      });
      return true;
    }

    // S2-T2: content-free UI signals for the activity ledger. Counts, domains and
    // durations only — never titles, text or anything the user is looking at.
    const uiQueue = [];
    const cardOpenTs = new WeakMap();
    let focusedSince = document.hasFocus() ? Date.now() : null;

    function queueUiEvent(kind, extra) {
      uiQueue.push(Object.assign({ kind }, extra || {}));
      if (uiQueue.length >= 40) flushUiEvents();
    }

    function flushUiEvents(unloading) {
      if (!sessionId || !uiQueue.length) return;
      const body = JSON.stringify({ events: uiQueue.splice(0, 200) });
      const url = `/api/sessions/${sessionId}/events`;
      try {
        if (unloading && navigator.sendBeacon) {
          navigator.sendBeacon(url, new Blob([body], { type: 'application/json' }));
          return;
        }
        fetch(url, {
          method: 'POST',
          credentials: 'include',
          headers: { 'Content-Type': 'application/json' },
          body,
          keepalive: !!unloading,
        }).catch(() => {});
      } catch (_) {
        /* telemetry must never break the UI */
      }
    }

    function wireUiSignals() {
      window.addEventListener('focus', () => {
        focusedSince = Date.now();
        queueUiEvent('ui_focus');
      });
      window.addEventListener('blur', () => {
        const spent = focusedSince ? Date.now() - focusedSince : null;
        focusedSince = null;
        queueUiEvent('ui_blur', spent ? { duration_ms: spent } : {});
      });
      document.addEventListener('click', (e) => {
        const card = e.target.closest && e.target.closest('.card');
        if (!card) return;
        const now = Date.now();
        if ((cardOpenTs.get(card) || 0) + 5000 > now) return; // dedupe drag/click bursts
        cardOpenTs.set(card, now);
        queueUiEvent('ui_card_open', { domain: card.dataset.domain || 'shared' });
      });
      window.addEventListener('agentrix:field-event', (e) => {
        const t = e.detail && e.detail.type;
        if (t === 'suzy_summary' || t === 'permission_request') queueUiEvent('ui_notification');
      });
      setInterval(() => flushUiEvents(), 20000);
      window.addEventListener('pagehide', () => flushUiEvents(true));
    }

    // S4-T8: per-agent authority modes for the badge layer. Anonymous sessions get a
    // 401/403 from the "known"-level route — badges simply stay off (no fabricated modes).
    async function fetchModes() {
      if (!sessionId) return;
      try {
        const res = await fetch(
          `/api/permissions?session_id=${encodeURIComponent(sessionId)}`,
          { credentials: 'include' }
        );
        if (!res.ok) return;
        const data = await res.json();
        const map = {};
        (data.agents || []).forEach((a) => {
          map[a.agent_id] = a.mode;
        });
        window.__AGENTRIX_MODES__ = map;
        if (ui.applyModeBadges) ui.applyModeBadges();
      } catch (_) {
        /* offline / not signed in */
      }
    }

    async function initSession() {
      const stored = sessionStorage.getItem(SESSION_KEY);
      if (stored && (await hydrateFromServer(stored))) return;
      await ensureSession();
    }

    function onVoiceIntent(ev) {
      const sid = ev.detail?.sessionId;
      const text = (ev.detail?.args?.text || '').trim();
      if (!text) return;
      if (sid) rememberSession(sid);
      window.__AGENTRIX_LIVE__?.submit?.(text);
    }

    async function apiSnooze(title, glyph, agent) {
      await ensureSession();
      const id = agent || title;
      await fetch(`/api/agents/${encodeURIComponent(id)}/snooze`, {
        method: 'POST',
        credentials: 'include',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ session_id: sessionId, title, glyph, agent: id }),
      });
    }

    async function apiWake(id, spec) {
      await ensureSession();
      await fetch(`/api/agents/${encodeURIComponent(id)}/wake`, {
        method: 'POST',
        credentials: 'include',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          session_id: sessionId,
          title: spec.title,
          glyph: spec.glyph,
          agent: spec.agent || id,
        }),
      });
    }

    async function apiCommit(cardTitle, actionLabel) {
      await ensureSession();
      await fetch(`/api/sessions/${sessionId}/commit`, {
        method: 'POST',
        credentials: 'include',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ card_title: cardTitle, action_label: actionLabel }),
      });
    }

    function wirePersistence() {
      if (ui.snoozeAgent) {
        const orig = ui.snoozeAgent;
        ui.snoozeAgent = function (title) {
          const card = [...document.querySelectorAll('.card')].find(
            (c) => c.dataset.title === title
          );
          const glyph = card?.querySelector('.glyph')?.textContent || '💤';
          apiSnooze(title, glyph, title).catch(() => {});
          return orig(title);
        };
      }
      if (ui.wakeAgent) {
        const orig = ui.wakeAgent;
        ui.wakeAgent = function (id, spec) {
          apiWake(id, spec).catch(() => {});
          return orig(id, spec);
        };
      }
      if (ui.commitAction) {
        const orig = ui.commitAction;
        ui.commitAction = function (cardEl, btn, label) {
          const title = cardEl?.dataset?.title || '';
          apiCommit(title, label).catch(() => {});
          return orig(cardEl, btn, label);
        };
      }
    }

    function beginScenario(key, text, totalCards) {
      ui.setCurrentKey(key);
      ui.setResolvedCount(0);
      ui.setTotalCards(totalCards);
      ui.chips.classList.add('hide');
      ui.suzy.classList.remove('show');
      ui.originText.textContent = text;
      ui.origin.classList.add('show');
      ui.exitFlow();
      ui.cardsEl.innerHTML = '';
      ui.resetAgents();
      cards.clear();
    }

    function spawnCard(index, spec, domain) {
      if (domain && !spec.domain) spec.domain = domain;
      const card = ui.buildCard(spec);
      ui.cardsEl.appendChild(card.el);
      card.el.animate(
        [
          { opacity: 0, transform: 'translateY(46px) scale(.92)' },
          { opacity: 1, transform: 'translateY(0) scale(1)' },
        ],
        {
          duration: 760,
          delay: index * 130,
          easing: 'cubic-bezier(.22,1,.36,1)',
          fill: 'backwards',
        }
      );
      ui.agentActive(spec);
      card.body.innerHTML = '';
      if (spec.surface) ui.buildSurface(card.body, spec.surface);
      const note = document.createElement('div');
      note.className = 'line sub';
      card.body.appendChild(note);
      cards.set(index, { card, spec, note });
      return card;
    }

    function updateStatus(index, status, line) {
      const entry = cards.get(index);
      if (!entry) return;
      if (line) entry.note.textContent = line;
      entry.card.stText.textContent =
        status === 'composing' ? 'composing…' : status === 'working' ? 'working…' : status;
      entry.card.status.className = 'status' + (status === 'working' ? ' working' : '');
    }

    async function runConductSteps(steps) {
      if (!steps?.length) return;
      ui.suzy.classList.remove('show');
      if (ui.origin?.querySelector('b')) {
        ui.origin.querySelector('b').textContent = 'performing your request…';
      }
      if (ui.hand) ui.hand.classList.add('show');

      for (const step of steps) {
        if (step.op === 'fuse' && step.source && step.target && ui.conductFuse) {
          await ui.conductFuse(step.source, step.target).catch(() => {});
        } else if (step.op === 'wait' && step.delay_ms) {
          await ui.wait(step.delay_ms);
        }
      }

      if (ui.hand) ui.hand.classList.remove('show');
      if (ui.input) ui.input.placeholder = 'What do you want to do?';
    }

    function handleEvent(ev, intentText) {
      // Phase 2 surfaces (chat panel, approvals inbox) observe the same stream.
      try {
        window.dispatchEvent(new CustomEvent('agentrix:field-event', { detail: ev }));
      } catch (_) {
        /* never let listeners break orchestration */
      }
      switch (ev.type) {
        case 'scenario':
          beginScenario(ev.key, ev.text || intentText, ev.total_cards || 0);
          break;
        case 'card_spawn':
          spawnCard(ev.index, ev.card, ev.domain);
          break;
        case 'card_status':
          updateStatus(ev.index, ev.status, ev.line);
          break;
        case 'card_surface': {
          const entry = cards.get(ev.index);
          if (!entry || ev.surface !== 'slides') break;
          const work = entry.card.body.querySelector('.work.slides');
          if (!work) break;
          const film = work.querySelector('.film');
          if (!film) break;
          const sls = film.querySelectorAll('.sl');
          const idx = Math.max(0, (ev.slide || 1) - 1);
          if (sls[idx]) {
            sls[idx].classList.add('in');
            sls[idx].textContent = String(ev.slide);
          }
          break;
        }
        case 'card_resolve': {
          const entry = cards.get(ev.index);
          if (!entry) break;
          const merged = Object.assign({}, entry.spec, { resolve: ev.resolve });
          ui.resolve(entry.card, merged, null);
          if (ev.resolve?.artifact_url) {
            const openBtn = entry.card.el.querySelector('.btn.primary');
            if (openBtn) {
              openBtn.addEventListener(
                'click',
                (e) => {
                  e.stopPropagation();
                  window.open(ev.resolve.artifact_url, '_blank');
                },
                { once: true }
              );
            }
          }
          break;
        }
        case 'conduct':
          return runConductSteps(ev.steps);
        case 'deck_finish':
          if (ui.finishDeck) {
            ui.finishDeck({
              big: ev.big,
              sub: ev.sub,
              artifact_url: ev.artifact_url,
              slide_count: ev.slide_count,
            });
          }
          break;
        case 'error':
          console.warn('[agentrix] orchestration error', ev.message);
          if (ui.showSuzyCustom) ui.showSuzyCustom(ev.message || 'Something went wrong.');
          break;
        case 'suzy_summary':
          ui.showSuzyCustom(ev.html, ev.audio_clip);
          break;
        case 'suggest':
          if (ui.armSuggestion) ui.armSuggestion(ev.text);
          break;
        case 'done':
          break;
        default:
          break;
      }
      return Promise.resolve();
    }

    async function consumeSse(res, intentText) {
      const reader = res.body.getReader();
      const dec = new TextDecoder();
      let buf = '';

      while (true) {
        const { done, value } = await reader.read();
        if (done) break;
        buf += dec.decode(value, { stream: true });
        const parts = buf.split('\n\n');
        buf = parts.pop() || '';
        for (const part of parts) {
          const line = part.split('\n').find((l) => l.startsWith('data: '));
          if (!line) continue;
          try {
            await handleEvent(JSON.parse(line.slice(6)), intentText);
          } catch (e) {
            console.warn('SSE parse error', e);
          }
        }
      }
    }

    async function submitLive(text) {
      const trimmed = text.trim();
      if (!trimmed) return;

      ui.clearSuggestion();
      ui.input.value = '';

      const hasCards = !!document.querySelector('.card');
      const isAction = ACTION_RE.test(trimmed);

      await ensureSession();
      if (abortController) abortController.abort();
      abortController = new AbortController();

      const endpoint = hasCards && isAction ? 'action' : 'intent';
      const res = await fetch(`/api/sessions/${sessionId}/${endpoint}`, {
        method: 'POST',
        credentials: 'include',
        headers: {
          'Content-Type': 'application/json',
          Accept: 'text/event-stream',
        },
        body: JSON.stringify({ text: trimmed }),
        signal: abortController.signal,
      });

      if (res.status === 403) {
        const needsAuth =
          endpoint === 'action'
            ? 'Actions like <b>combine</b> need sign-in — use <b>Dev sign-in</b> (top right) or Google.'
            : 'This request needs a higher trust level — sign in to continue.';
        if (ui.showSuzyCustom) ui.showSuzyCustom(needsAuth);
        return;
      }
      if (!res.ok) {
        const detail = await res.text().catch(() => '');
        throw new Error(`${endpoint} failed: ${res.status}${detail ? ` — ${detail}` : ''}`);
      }

      await consumeSse(res, trimmed);
    }

    // S10-T3: a conversational turn with the Mother Agent. Same SSE pipeline as an
    // intent (cards bloom in the field), plus the turn lands in the session chat history.
    async function submitChat(text) {
      const trimmed = text.trim();
      if (!trimmed) return;
      await ensureSession();
      if (abortController) abortController.abort();
      abortController = new AbortController();
      const res = await fetch(`/api/sessions/${sessionId}/chat`, {
        method: 'POST',
        credentials: 'include',
        headers: {
          'Content-Type': 'application/json',
          Accept: 'text/event-stream',
        },
        body: JSON.stringify({ text: trimmed }),
        signal: abortController.signal,
      });
      if (!res.ok) {
        const detail = await res.text().catch(() => '');
        throw new Error(`chat failed: ${res.status}${detail ? ` — ${detail}` : ''}`);
      }
      await consumeSse(res, trimmed);
    }

    window.__AGENTRIX_LIVE__ = {
      submit(text) {
        submitLive(text).catch((err) => {
          if (err.name === 'AbortError') return;
          console.error('[agentrix] live intent failed', err);
          const msg = `Could not reach the server — ${err.message || 'try again'}.`;
          if (ui.showSuzyCustom) ui.showSuzyCustom(msg);
          else alert(msg);
        });
      },
      chat(text) {
        return submitChat(text);
      },
      ensureSession,
      getSessionId() {
        return sessionId;
      },
      recordUiEvent(kind, extra) {
        queueUiEvent(kind, extra);
      },
    };

    wirePersistence();
    wireUiSignals();
    window.addEventListener('agentrix:voice-intent', onVoiceIntent);
    initSession()
      .catch(() => ensureSession().catch(() => {}))
      .then(() => fetchModes());
    console.info('[agentrix] live mode — SSE orchestration + persistence enabled');
  }

  boot().catch((err) => console.warn('[agentrix] field-client boot failed', err));
})();