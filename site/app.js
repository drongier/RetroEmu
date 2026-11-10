// Colle : le scroll pilote la scène, un clic ouvre la fiche du composant.
(function () {
  const GB = window.GB;
  const scene = GB.scene;
  const $ = id => document.getElementById(id);
  const root = document.documentElement;
  const track = $('track'), panel = $('panel'), body = $('panel-body');
  const reduced = window.matchMedia('(prefers-reduced-motion: reduce)').matches;

  /** Début de chaque étape, et point d'arrêt où la vue est stable, en fraction du scroll. */
  const STARTS = [0, 0.14, 0.34, 0.52, 0.68, 0.84, 0.955];
  const STOPS = [0, 0.26, 0.45, 0.615, 0.775, 0.91, 1];

  if (!scene.ok) root.classList.add('no3d');

  // ───────────────────────────── Scroll ─────────────────────────────

  const range = () => Math.max(1, track.offsetHeight - window.innerHeight);
  const progress = () => Math.min(1, Math.max(0, -track.getBoundingClientRect().top / range()));
  function goTo(p, instant) {
    const top = track.getBoundingClientRect().top + window.scrollY + p * range();
    window.scrollTo({ top, behavior: instant || reduced ? 'auto' : 'smooth' });
  }

  const nav = $('stages');
  GB.stages.forEach((s, i) => {
    const li = document.createElement('li');
    li.innerHTML = `<button type="button"><b>${i + 1}</b><span>${s.nav}</span></button>`;
    li.firstChild.addEventListener('click', () => goTo(STOPS[i]));
    nav.appendChild(li);
  });

  let stage = -1;
  function setStage(i) {
    stage = i;
    const s = GB.stages[i];
    $('cap-kicker').textContent = s.kicker;
    $('cap-title').textContent = s.title;
    $('cap-text').textContent = s.text;
    $('caption').dataset.stage = i;
    [...nav.children].forEach((li, k) => li.classList.toggle('on', k === i));
    $('hint').hidden = i !== 0;
  }

  let queued = false, jump = reduced;
  function onScroll() {
    if (queued) return;
    queued = true;
    requestAnimationFrame(() => {
      queued = false;
      const p = progress();
      let i = STARTS.length - 1;
      while (i > 0 && p < STARTS[i]) i--;
      if (i !== stage) setStage(i);
      $('bar').style.transform = `scaleX(${p})`;
      if (scene.ok) scene.setProgress(p, i, jump);
      jump = reduced;
    });
  }
  window.addEventListener('scroll', onScroll, { passive: true });
  window.addEventListener('resize', onScroll);

  // ───────────────────────────── Fiche ─────────────────────────────

  let cleanup = null, current = null;
  const wide = () => window.innerWidth > 820;

  function open(id, o) {
    const c = GB.components[id];
    if (!c) return;
    o = o || {};
    if (cleanup) cleanup();
    current = id;
    $('panel-kicker').textContent = GB.stages[c.stage].nav + ' · ' + c.hw;
    $('panel-title').textContent = c.name;
    $('panel-meta').innerHTML =
      `<div><dt>Dans RetroEmu</dt><dd><code>${c.src}</code></dd></div>` + (c.addr ? `<div><dt>Adresses</dt><dd><code>${c.addr}</code></dd></div>` : '');
    body.innerHTML = c.html;
    GB.hydrate(body);
    cleanup = c.init ? c.init(body) || null : null;
    const k = GB.order.indexOf(id), n = GB.order.length;
    const prev = GB.components[GB.order[(k + n - 1) % n]], next = GB.components[GB.order[(k + 1) % n]];
    $('panel-prev').textContent = '← ' + prev.name;
    $('panel-prev').dataset.open = prev.id;
    $('panel-next').textContent = next.name + ' →';
    $('panel-next').dataset.open = next.id;
    panel.hidden = false;
    root.classList.add('panel-open');
    body.scrollTop = 0;
    if (scene.ok) {
      scene.select(id);
      scene.setPanel(wide() ? panel.offsetWidth : 0);
    }
    if (o.scroll) {
      jump = jump || !!o.instant;
      goTo(STOPS[c.stage], o.instant);
    }
    try {
      history.replaceState(null, '', '#' + id);
    } catch (e) {}
  }

  function close() {
    if (cleanup) cleanup();
    cleanup = current = null;
    panel.hidden = true;
    root.classList.remove('panel-open');
    body.innerHTML = '';
    if (scene.ok) {
      scene.select(null);
      scene.setPanel(0);
    }
    try {
      history.replaceState(null, '', '#');
    } catch (e) {}
  }

  scene.onPick = id => open(id);
  $('panel-close').addEventListener('click', close);
  window.addEventListener('keydown', e => {
    if (e.key === 'Escape' && current) close();
  });
  window.addEventListener('resize', () => {
    if (current && scene.ok) scene.setPanel(wide() ? panel.offsetWidth : 0);
  });
  document.addEventListener('click', e => {
    const t = e.target.closest('[data-open], [data-stage]');
    if (!t) return;
    if (t.dataset.open) {
      open(t.dataset.open, { scroll: true });
    } else {
      close();
      goTo(STOPS[+t.dataset.stage]);
    }
  });

  // ───────────────────────────── Liste complète ─────────────────────────────

  const index = $('index-list');
  [2, 3, 4, 5].forEach(i => {
    const items = GB.order.map(id => GB.components[id]).filter(c => c.stage === i);
    const group = document.createElement('section');
    group.className = 'group';
    group.innerHTML =
      `<h3>${GB.stages[i].nav}</h3><div class="cards">` +
      items.map(c => `<button type="button" class="card" data-open="${c.id}"><b>${c.name}</b><span>${c.short}</span><code>${c.src}</code></button>`).join('') +
      '</div>';
    index.appendChild(group);
  });

  onScroll();
  setStage(0);
  const hash = location.hash.slice(1);
  if (GB.components[hash]) open(hash, { scroll: true, instant: true });
})();
