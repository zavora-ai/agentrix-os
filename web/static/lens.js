/**
 * Worlds pager (S10-T2, evolved): Work ‹ Both › Home, each world a full window with
 * its own colour theme (body[data-world] retints the CSS vars and aurora). Navigate by
 * horizontal swipe, trackpad scroll, arrow keys, or the pager buttons. Filtering hides
 * the other world's cards, tiles and approval rows; shared items show everywhere
 * (ADR-002). Persisted in localStorage until preferences move server-side (S10); every
 * switch records a content-free ui_lens ledger event. Live mode only.
 */
(function () {
  'use strict';

  const KEY = 'agentrix_lens';
  const ORDER = ['work', 'both', 'home'];
  const TITLES = { work: 'Work World', home: 'Home World' };

  async function boot() {
    if (window.__AGENTRIX_BOOT__) await window.__AGENTRIX_BOOT__;
    if (window.__AGENTRIX_DEMO__) return;
    if (localStorage.getItem('agentrix_p2') === '0') return;

    const ctl = document.getElementById('lensCtl');
    const title = document.getElementById('worldTitle');
    if (!ctl) return;
    const btns = [...ctl.querySelectorAll('button')];
    let current = 'both';

    function set(world, record) {
      if (!ORDER.includes(world)) return;
      if (world !== current) {
        const dir = ORDER.indexOf(world) > ORDER.indexOf(current) ? 'world-anim-l' : 'world-anim-r';
        document.body.classList.remove('world-anim-l', 'world-anim-r');
        void document.body.offsetWidth; // restart the slide animation
        document.body.classList.add(dir);
        setTimeout(() => document.body.classList.remove('world-anim-l', 'world-anim-r'), 620);
      }
      current = world;
      if (world === 'both') delete document.body.dataset.world;
      else document.body.dataset.world = world;
      btns.forEach((b) => b.classList.toggle('on', b.dataset.lens === world));
      if (title) {
        if (TITLES[world]) {
          title.textContent = TITLES[world];
          title.removeAttribute('hidden');
        } else {
          title.setAttribute('hidden', '');
        }
      }
      try {
        localStorage.setItem(KEY, world);
      } catch (_) {
        /* private browsing */
      }
      if (record) {
        window.__AGENTRIX_LIVE__?.recordUiEvent?.('ui_lens', {
          domain: world === 'both' ? 'shared' : world,
        });
      }
    }

    // +1 moves right in the pager (toward Home), -1 toward Work
    function step(dir) {
      const idx = ORDER.indexOf(current) + dir;
      if (idx < 0 || idx >= ORDER.length) return;
      set(ORDER[idx], true);
    }

    // Other surfaces (camera gestures, M10-T5) drive the pager through this.
    window.__AGENTRIX_LENS__ = { step, set: (w) => set(w, true), current: () => current };

    btns.forEach((b) => b.addEventListener('click', () => set(b.dataset.lens, true)));

    // Interactive surfaces keep their own gestures (card drag/fling, panels, feeds).
    const skip = (el) =>
      el &&
      el.closest &&
      el.closest('.card, .bgcard, .p2-panel, .lens, .rail, .livefeed, .horizon, .greet, .ea, input, textarea, button');

    // touch swipe: content follows the finger direction (swipe left → Home side)
    let tx = null;
    let ty = null;
    document.addEventListener(
      'touchstart',
      (e) => {
        if (skip(e.target)) {
          tx = null;
          return;
        }
        tx = e.touches[0].clientX;
        ty = e.touches[0].clientY;
      },
      { passive: true }
    );
    document.addEventListener(
      'touchend',
      (e) => {
        if (tx == null) return;
        const dx = e.changedTouches[0].clientX - tx;
        const dy = e.changedTouches[0].clientY - ty;
        tx = null;
        if (Math.abs(dx) > 70 && Math.abs(dx) > Math.abs(dy) * 1.5) step(dx < 0 ? 1 : -1);
      },
      { passive: true }
    );

    // trackpad: two-finger horizontal scroll
    let acc = 0;
    let accAt = 0;
    window.addEventListener(
      'wheel',
      (e) => {
        if (skip(e.target)) return;
        if (Math.abs(e.deltaX) <= Math.abs(e.deltaY)) return;
        const now = Date.now();
        if (now - accAt > 400) acc = 0;
        accAt = now;
        acc += e.deltaX;
        if (Math.abs(acc) > 160) {
          step(acc > 0 ? 1 : -1);
          acc = 0;
        }
      },
      { passive: true }
    );

    // arrow keys (unless typing)
    window.addEventListener('keydown', (e) => {
      if (e.key !== 'ArrowLeft' && e.key !== 'ArrowRight') return;
      const a = document.activeElement;
      if (a && (a.tagName === 'INPUT' || a.tagName === 'TEXTAREA' || a.isContentEditable)) return;
      step(e.key === 'ArrowRight' ? 1 : -1);
    });

    const saved = localStorage.getItem(KEY);
    if (saved === 'work' || saved === 'home') set(saved, false);

    ctl.removeAttribute('hidden');
  }

  boot().catch((err) => console.warn('[agentrix] worlds pager boot failed', err));
})();
