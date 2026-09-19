/**
 * Approvals inbox v1 (S10-T4): pending actions queued by the permission gate (S2).
 * Lists GET /api/actions?status=pending with live expiry countdowns, approve / reject /
 * edit and batch approve, plus a "while you were away" feed from GET /api/audit
 * (full viewer lands in S11-T3). Reacts live to permission_request / action_result SSE
 * events. Known-level routes: signed-out sessions see a sign-in hint, never fabricated
 * data. Live mode only; disable with localStorage agentrix_p2=0.
 */
(function () {
  'use strict';

  const EFFECT_HELP = {
    read: 'reads data only',
    write_local: 'creates or edits drafts and files inside your OS',
    schedule_with_others: 'books time with other people',
    send_external: 'sends email or messages outside your OS',
    publish_public: 'posts publicly — always needs your approval',
    financial: 'moves money — never automated',
    delete: 'archives or deletes items',
  };
  const DECISION_ICONS = {
    approved: '✓',
    allowed: '✓',
    rejected: '✕',
    denied: '✕',
    failed: '⚠',
    queued: '⧖',
    paused: '⏸',
  };

  function toast(msg) {
    const host = document.getElementById('p2Toasts');
    if (!host) return;
    const t = document.createElement('div');
    t.className = 'p2-toast';
    t.textContent = msg;
    host.appendChild(t);
    setTimeout(() => t.remove(), 6000);
  }
  window.__AGENTRIX_P2_TOAST__ = toast;

  function expiresIn(iso) {
    const ms = new Date(iso).getTime() - Date.now();
    if (!isFinite(ms)) return '';
    if (ms <= 0) return 'expired';
    const h = Math.floor(ms / 3600000);
    const m = Math.floor((ms % 3600000) / 60000);
    return h >= 1 ? `expires in ${h}h ${m}m` : `expires in ${Math.max(1, m)}m`;
  }

  function timeAgo(iso) {
    const ms = Date.now() - new Date(iso).getTime();
    if (!isFinite(ms) || ms < 0) return '';
    const min = Math.floor(ms / 60000);
    if (min < 1) return 'just now';
    if (min < 60) return `${min}m ago`;
    const h = Math.floor(min / 60);
    if (h < 24) return `${h}h ago`;
    return `${Math.floor(h / 24)}d ago`;
  }

  async function boot() {
    if (window.__AGENTRIX_BOOT__) await window.__AGENTRIX_BOOT__;
    if (window.__AGENTRIX_DEMO__) return;
    if (localStorage.getItem('agentrix_p2') === '0') return;

    const openBtn = document.getElementById('apprOpen');
    const panel = document.getElementById('apprPanel');
    const list = document.getElementById('apprList');
    const countEl = document.getElementById('apprCount');
    const allBtn = document.getElementById('apprAll');
    const closeBtn = document.getElementById('apprClose');
    const refreshBtn = document.getElementById('apprRefresh');
    if (!openBtn || !panel || !list || !countEl || !allBtn) return;

    let items = [];
    let signedOut = false;
    let lastCount = 0;

    const sid = () => window.__AGENTRIX_LIVE__?.getSessionId?.() || null;
    const qs = () => {
      const s = sid();
      return s ? `&session_id=${encodeURIComponent(s)}` : '';
    };

    function setBadge(n) {
      if (n > 0) {
        countEl.textContent = String(n);
        countEl.removeAttribute('hidden');
        if (n > lastCount) {
          openBtn.classList.remove('pulse');
          void openBtn.offsetWidth; // restart the animation
          openBtn.classList.add('pulse');
          setTimeout(() => openBtn.classList.remove('pulse'), 2600);
        }
      } else {
        countEl.setAttribute('hidden', '');
      }
      lastCount = n;
    }

    function auditSection(entries) {
      if (!entries.length) return;
      const div = document.createElement('div');
      div.className = 'appr-divider';
      div.textContent = 'While you were away';
      list.appendChild(div);
      entries.forEach((a) => {
        const row = document.createElement('div');
        row.className = 'audit-row';
        const ic = document.createElement('span');
        ic.className = 'ic';
        ic.textContent = DECISION_ICONS[a.decision] || '·';
        const sum = document.createElement('span');
        sum.className = 'sum';
        sum.textContent = a.summary || `${a.agent_id} · ${a.tool}`;
        const when = document.createElement('span');
        when.className = 'when';
        when.textContent = timeAgo(a.created_at);
        row.append(ic, sum, when);
        list.appendChild(row);
      });
    }

    async function loadAudit() {
      try {
        const res = await fetch(`/api/audit?limit=6${qs()}`, { credentials: 'include' });
        if (!res.ok) return;
        const data = await res.json();
        auditSection(data.entries || []);
      } catch (_) {
        /* offline */
      }
    }

    function render() {
      list.innerHTML = '';
      allBtn.setAttribute('hidden', '');
      if (signedOut) {
        list.innerHTML =
          '<div class="p2-empty">Sign in (top right) to review actions waiting for your approval.</div>';
        return;
      }
      if (!items.length) {
        list.innerHTML = '<div class="p2-empty">Nothing waiting for you — agents will queue actions here.</div>';
      }
      if (items.length > 1) allBtn.removeAttribute('hidden');
      items.forEach((a) => {
        const row = document.createElement('div');
        row.className = 'appr-row';
        if (a.domain === 'work' || a.domain === 'home') row.classList.add('dom-' + a.domain);
        const meta = document.createElement('div');
        meta.className = 'appr-meta';
        meta.innerHTML = `<span class="agent-chip"></span><span class="effect-chip ${a.effect}"></span><span class="appr-when"></span>`;
        meta.querySelector('.agent-chip').textContent = a.agent_id;
        const chip = meta.querySelector('.effect-chip');
        chip.textContent = a.effect;
        chip.title = EFFECT_HELP[a.effect] || '';
        const when = meta.querySelector('.appr-when');
        when.dataset.expires = a.expires_at;
        when.textContent = expiresIn(a.expires_at);
        const sum = document.createElement('div');
        sum.className = 'appr-sum';
        sum.textContent = a.summary || `${a.agent_id} wants to run ${a.tool}`;
        const btns = document.createElement('div');
        btns.className = 'appr-btns';
        const mk = (label, cls, fn) => {
          const b = document.createElement('button');
          b.className = 'btn ' + cls;
          b.textContent = label;
          b.setAttribute('aria-label', `${label}: ${a.summary || a.tool}`);
          b.addEventListener('click', fn);
          btns.appendChild(b);
          return b;
        };
        mk('Approve', 'primary approve', () => act(a, 'approve', row));
        mk('Edit', '', () => edit(a));
        mk('Reject', '', () => act(a, 'reject', row));
        row.appendChild(meta);
        row.appendChild(sum);
        row.appendChild(btns);
        list.appendChild(row);
      });
      loadAudit();
    }

    // tick the countdowns while the panel is open
    setInterval(() => {
      if (panel.hidden) return;
      list.querySelectorAll('.appr-when[data-expires]').forEach((el) => {
        el.textContent = expiresIn(el.dataset.expires);
      });
    }, 30000);

    async function load() {
      try {
        await window.__AGENTRIX_LIVE__?.ensureSession?.();
        const res = await fetch(`/api/actions?status=pending${qs()}`, { credentials: 'include' });
        if (res.status === 401 || res.status === 403) {
          signedOut = true;
          items = [];
        } else if (res.ok) {
          signedOut = false;
          const data = await res.json();
          items = data.actions || [];
        } else {
          return; // transient error: keep the previous view
        }
        setBadge(items.length);
        render();
      } catch (_) {
        /* offline: keep previous view */
      }
    }

    async function act(a, verb, row) {
      try {
        const res = await fetch(`/api/actions/${a.id}/${verb}`, {
          method: 'POST',
          credentials: 'include',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ session_id: sid() }),
        });
        if (!res.ok) {
          toast(`Could not ${verb} — ${res.status}`);
          return;
        }
        const updated = await res.json();
        toast(
          updated.status === 'approved'
            ? `Approved — ${a.summary}`
            : updated.status === 'failed'
              ? `Failed — ${a.summary}`
              : `Rejected — ${a.summary}`
        );
        if (row) {
          row.classList.add('appr-out');
          setBadge(Math.max(0, lastCount - 1));
          setTimeout(load, 320);
          return;
        }
      } catch (err) {
        toast(`Could not ${verb} — ${err.message || 'try again'}`);
      }
      load();
    }

    async function edit(a) {
      const current = JSON.stringify(a.args ?? {}, null, 2);
      const next = window.prompt(`Edit arguments for ${a.tool} (JSON):`, current);
      if (next == null) return;
      let args;
      try {
        args = JSON.parse(next);
      } catch (_) {
        toast('Edit cancelled — that was not valid JSON.');
        return;
      }
      try {
        const res = await fetch(`/api/actions/${a.id}/edit`, {
          method: 'POST',
          credentials: 'include',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ session_id: sid(), args }),
        });
        toast(res.ok ? `Updated — ${a.summary}` : `Could not edit — ${res.status}`);
      } catch (err) {
        toast(`Could not edit — ${err.message || 'try again'}`);
      }
      load();
    }

    async function approveAll() {
      const ids = items.map((a) => a.id);
      if (!ids.length) return;
      try {
        const res = await fetch('/api/actions/approve', {
          method: 'POST',
          credentials: 'include',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ ids, session_id: sid() }),
        });
        if (res.ok) {
          const data = await res.json();
          toast(`Approved ${(data.approved || []).length} action(s)`);
        } else {
          toast(`Batch approve failed — ${res.status}`);
        }
      } catch (err) {
        toast(`Batch approve failed — ${err.message || 'try again'}`);
      }
      load();
    }

    function open() {
      document.getElementById('chatPanel')?.setAttribute('hidden', '');
      panel.removeAttribute('hidden');
      load();
    }
    function close() {
      panel.setAttribute('hidden', '');
    }
    openBtn.addEventListener('click', () => (panel.hidden ? open() : close()));
    if (closeBtn) closeBtn.addEventListener('click', close);
    if (refreshBtn) refreshBtn.addEventListener('click', load);
    allBtn.addEventListener('click', approveAll);

    window.addEventListener('agentrix:field-event', (e) => {
      const ev = e.detail || {};
      if (ev.type === 'permission_request') {
        toast(`Needs your approval: ${ev.summary}`);
        load();
      } else if (ev.type === 'action_result') {
        load();
      }
    });

    openBtn.removeAttribute('hidden');
    // First load after field-client has had a chance to restore the session.
    setTimeout(load, 1500);
  }

  boot().catch((err) => console.warn('[agentrix] approvals boot failed', err));
})();
