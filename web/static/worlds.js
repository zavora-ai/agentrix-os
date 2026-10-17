/**
 * World rosters (concept §4–§5): the Work world lists its eight agents, the Home world its eight,
 * from GET /api/worlds. Rendered as tiles at the top of the right rail with the same `dom-*`
 * classes the lens filter already hides and shows, so Work shows only work agents, Home only home
 * agents, and Both shows both groups. Clicking a tile types that agent's starter prompt into the
 * intent bar. Live mode only; the demo tour keeps its static rails.
 */
(function () {
  'use strict';

  const MODE_ICON = { observe: '👁', suggest: '💡', automate: '⚡' };

  function tile(a) {
    const t = document.createElement('div');
    t.className = `tile roster dom-${a.world}${a.stub ? ' stub' : ''}`;
    t.dataset.agentId = a.id;
    t.dataset.world = a.world;
    t.dataset.title = a.title;
    t.dataset.agent = a.phase1_agents?.[0] || '';
    t.title = a.mission + (a.stub ? ' — labeled stub until its integration exists' : '');
    const small = a.stub ? 'labeled stub' : `${MODE_ICON[a.mode] || ''} ${a.mode}`.trim();
    const { esc } = window.AgentrixHtml;
    t.innerHTML = `<div class="av">${esc(a.glyph)}</div><div class="who">${esc(a.title)}<small>${esc(small)}</small></div>`;
    t.addEventListener('click', () => {
      const input = document.getElementById('intent');
      if (!input) return;
      input.value = a.prompt;
      input.focus();
      window.__AGENTRIX_UI__?.showSuzyCustom?.(`${esc(a.glyph)} <b>${esc(a.title)}</b> — ${esc(a.mission)} Press ⏎ to ask.`);
    });
    return t;
  }

  function group(world, agents, label) {
    const h = document.createElement('h3');
    h.className = `world-roster-h dom-${world}`;
    h.textContent = label;
    const g = document.createElement('div');
    g.className = `grp world-roster dom-${world}`;
    g.id = `world${world[0].toUpperCase()}${world.slice(1)}`;
    agents.forEach((a) => g.appendChild(tile(a)));
    return [h, g];
  }

  async function boot() {
    if (window.__AGENTRIX_BOOT__) await window.__AGENTRIX_BOOT__;
    if (window.__AGENTRIX_DEMO__) return;
    if (localStorage.getItem('agentrix_p2') === '0') return;
    const rail = document.querySelector('.rail.right');
    if (!rail) return;
    let data;
    try {
      const res = await fetch('/api/worlds', { credentials: 'include' });
      if (!res.ok) return;
      data = await res.json();
    } catch (_) {
      return;
    }
    const frag = document.createDocumentFragment();
    group('work', data.work || [], 'Work agents').forEach((el) => frag.appendChild(el));
    group('home', data.home || [], 'Home agents').forEach((el) => frag.appendChild(el));
    rail.insertBefore(frag, rail.firstChild);
    window.__AGENTRIX_WORLDS__ = data;
  }

  boot().catch((err) => console.warn('[agentrix] world rosters boot failed', err));
})();
