// Contenu pédagogique : une fiche par composant, avec ses widgets « à manipuler ».
// Tout ce qui est écrit ici décrit le code de src/ : si le code change, la fiche doit suivre.
(function () {
  const GB = (window.GB = window.GB || {});

  const hx = (n, w = 2) => n.toString(16).toUpperCase().padStart(w, '0');
  const bin = (n, w = 8) => n.toString(2).padStart(w, '0');
  const nib = n => bin(n).replace(/(....)(....)/, '$1 $2');
  const esc = s => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
  const $ = (root, sel) => root.querySelector(sel);

  /** Extrait de code Rust, avec les commentaires atténués. */
  const rs = (file, code) =>
    `<figure class="code"><figcaption>${file}</figcaption><pre><code>${esc(code.replace(/^\n+|\s+$/g, '')).replace(
      /(\/\/.*)$/gm,
      '<i>$1</i>'
    )}</code></pre></figure>`;

  /** Bande de bits statique : remplie par `hydrate` à l'ouverture de la fiche. */
  const st = (value, labels, groups, n = 8) =>
    `<div class="strip" data-v="${value}" data-n="${n}" data-labels="${(labels || []).join('|')}" data-groups="${(groups || []).join(',')}"></div>`;

  /** Construit une bande de bits dans `el`. `editable` est un masque des bits cliquables. */
  function buildStrip(el, o) {
    const n = o.n || 8;
    let v = o.value || 0;
    const editable = o.editable || 0;
    el.classList.add('strip');
    el.style.setProperty('--n', n);
    el.innerHTML = '';
    const cells = {};
    for (let b = n - 1; b >= 0; b--) {
      const i = n - 1 - b;
      const cell = document.createElement('div');
      const g = o.groups && o.groups[i] ? ' g' + o.groups[i] : '';
      cell.className = 'cell' + g;
      const can = (editable >> b) & 1;
      const lab = o.labels && o.labels[i] ? o.labels[i] : '';
      cell.innerHTML =
        `<span class="idx">${b}</span>` +
        (can ? `<button type="button" class="bit" aria-label="bit ${b}"></button>` : `<span class="bit"></span>`) +
        `<span class="lab">${lab}</span>`;
      const bit = cell.querySelector('.bit');
      if (can) {
        bit.addEventListener('click', () => {
          v ^= 1 << b;
          paint();
          if (o.onChange) o.onChange(v);
        });
      }
      cells[b] = { cell, bit };
      el.appendChild(cell);
    }
    function paint() {
      for (const b in cells) {
        const on = (v >> b) & 1;
        cells[b].bit.textContent = on;
        cells[b].bit.classList.toggle('on', !!on);
      }
    }
    paint();
    return {
      get: () => v,
      set: x => {
        v = x;
        paint();
      },
      cell: b => cells[b].cell,
    };
  }

  GB.hydrate = function (root) {
    root.querySelectorAll('.strip[data-v]').forEach(el => {
      buildStrip(el, {
        n: +el.dataset.n,
        value: +el.dataset.v,
        labels: el.dataset.labels ? el.dataset.labels.split('|') : null,
        groups: el.dataset.groups ? el.dataset.groups.split(',').map(Number) : null,
      });
      el.removeAttribute('data-v');
    });
  };

  const hexOf = input => {
    const v = parseInt(input.value, 16);
    return Number.isNaN(v) ? null : v;
  };

  // ───────────────────────────── Widgets ─────────────────────────────

  function initCpu(root) {
    // Additionneur : A (op) n, et les 4 drapeaux.
    const a = $(root, '#alu-a'), b = $(root, '#alu-b'), op = $(root, '#alu-op'), out = $(root, '#alu-out');
    const flags = buildStrip($(root, '#alu-flags'), { labels: ['Z', 'N', 'H', 'C', '0', '0', '0', '0'], groups: [1, 2, 3, 4] });
    function alu() {
      const x = hexOf(a), y = hexOf(b);
      if (x === null || y === null) return;
      const sub = op.value === 'sub';
      const r = (sub ? x - y : x + y) & 0xff;
      const H = sub ? (x & 15) < (y & 15) : (x & 15) + (y & 15) > 15;
      const C = sub ? x < y : x + y > 255;
      flags.set(((r === 0) << 7) | (sub << 6) | (H << 5) | (C << 4));
      const lo = sub
        ? `0x${hx(x & 15, 1)} − 0x${hx(y & 15, 1)} ${H ? 'demande un emprunt' : 'passe sans emprunt'}`
        : `0x${hx(x & 15, 1)} + 0x${hx(y & 15, 1)} = 0x${hx((x & 15) + (y & 15))} ${H ? '> 0xF' : '≤ 0xF'}`;
      out.innerHTML =
        `<pre class="calc">  A   ${nib(x)}   0x${hx(x)}\n${sub ? '−' : '+'} n   ${nib(y)}   0x${hx(y)}\n  =   ${nib(r)}   0x${hx(r)}</pre>` +
        `<p class="res">Quartets bas : ${lo} → <b>H = ${+H}</b>. ` +
        (sub ? `A ${C ? '&lt;' : '≥'} n → <b>C = ${+C}</b>.` : `Somme ${C ? '&gt;' : '≤'} 0xFF → <b>C = ${+C}</b>.`) +
        ` Résultat ${r === 0 ? 'nul' : 'non nul'} → <b>Z = ${+(r === 0)}</b>.</p>`;
    }
    [a, b, op].forEach(el => el.addEventListener('input', alu));
    alu();

    // Décodeur d'opcode : les blocs 01 ddd sss et 10 ooo sss.
    const R = ['B', 'C', 'D', 'E', 'H', 'L', '(HL)', 'A'];
    const OPS = ['ADD', 'ADC', 'SUB', 'SBC', 'AND', 'XOR', 'OR', 'CP'];
    const range = $(root, '#op-range'), opOut = $(root, '#op-out');
    const strip = buildStrip($(root, '#op-strip'), { groups: [1, 1, 2, 2, 2, 3, 3, 3], labels: ['bloc', 'bloc', 'y', 'y', 'y', 'z', 'z', 'z'] });
    function decode() {
      const o = +range.value;
      strip.set(o);
      const y = (o >> 3) & 7, z = o & 7;
      let text, cycles;
      if (o === 0x76) {
        text = '<b>HALT</b> : la case qui serait « LD (HL), (HL) ». Son bras passe avant la plage des LD dans le <code>match</code>.';
        cycles = 4;
      } else if (o >> 6 === 1) {
        text = `<b>LD ${R[y]}, ${R[z]}</b> : bloc 01, destination y = ${y}, source z = ${z}.`;
        cycles = y === 6 || z === 6 ? 8 : 4;
      } else {
        text = `<b>${OPS[y]} A, ${R[z]}</b> : bloc 10, opération y = ${y}, source z = ${z}.`;
        cycles = z === 6 ? 8 : 4;
      }
      opOut.innerHTML = `<span class="big">0x${hx(o)}</span> ${text} <span class="tagline">${cycles} T-cycles</span>`;
    }
    range.addEventListener('input', decode);
    decode();
  }

  function initInterrupts(root) {
    const NAMES = ['VBlank', 'STAT', 'Timer', 'Série', 'Joypad'];
    const labels = ['–', '–', '–', 'Joypad', 'Série', 'Timer', 'STAT', 'VBlank'];
    const groups = [0, 0, 0, 1, 2, 3, 4, 1];
    const out = $(root, '#int-out'), ime = $(root, '#int-ime');
    const ie = buildStrip($(root, '#int-ie'), { value: 0x05, editable: 0x1f, labels, groups, onChange: run });
    const iflag = buildStrip($(root, '#int-if'), { value: 0x04, editable: 0x1f, labels, groups, onChange: run });
    function run() {
      const pending = ie.get() & iflag.get() & 0x1f;
      if (!pending) {
        out.innerHTML = `IE &amp; IF = <code>${bin(pending, 5)}</code> : aucune interruption à la fois demandée et autorisée. Le CPU continue son programme.`;
        return;
      }
      const bit = Math.clz32(pending & -pending) ^ 31;
      if (!ime.checked) {
        out.innerHTML = `IE &amp; IF = <code>${bin(pending, 5)}</code> : <b>${NAMES[bit]}</b> attend, mais IME = 0. Rien n'est servi. Un CPU en HALT se réveille quand même.`;
        return;
      }
      out.innerHTML =
        `IE &amp; IF = <code>${bin(pending, 5)}</code>. Bit à 1 le plus bas : <b>${bit} (${NAMES[bit]})</b>.<br>` +
        `Le CPU efface le bit ${bit} de IF, passe IME à 0, empile PC, puis saute en <b>0x${hx(0x40 + 8 * bit, 4)}</b> (0x40 + 8 × ${bit}). Coût : 20 T-cycles.`;
    }
    ime.addEventListener('change', run);
    run();
  }

  function initTimer(root) {
    const BITS = [9, 3, 5, 7];
    const s = { counter: 0, tima: 0xf8, tma: 0xf8, tac: 0x05 };
    let running = !window.matchMedia('(prefers-reduced-motion: reduce)').matches;
    let raf = 0, marked = -1;
    const groups = [1, 1, 1, 1, 1, 1, 1, 1];
    const counter = buildStrip($(root, '#tm-counter'), { n: 16, groups });
    const tac = buildStrip($(root, '#tm-tac'), {
      value: s.tac, editable: 0x07, labels: ['–', '–', '–', '–', '–', 'marche', 'vitesse', 'vitesse'], groups: [0, 0, 0, 0, 0, 3, 2, 2],
      onChange: v => {
        const before = signal();
        s.tac = v & 7;
        fall(before, 'écriture dans TAC');
        paint();
      },
    });
    const vals = $(root, '#tm-vals'), log = $(root, '#tm-log'), play = $(root, '#tm-play'), speed = $(root, '#tm-speed');
    const signal = () => (s.tac & 4) !== 0 && ((s.counter >> BITS[s.tac & 3]) & 1) === 1;
    function fall(before, why) {
      if (!before || signal()) return;
      if (s.tima === 0xff) {
        s.tima = s.tma;
        log.innerHTML = `Débordement : TIMA reçoit TMA (0x${hx(s.tma)}) et le timer demande l'<b>interruption Timer</b> (bit 2 de IF).`;
      } else {
        s.tima++;
        if (why) log.innerHTML = `Front descendant provoqué par l'${why} : TIMA avance sans attendre son tour.`;
      }
    }
    function step() {
      const before = signal();
      s.counter = (s.counter + 4) & 0xffff;
      fall(before);
    }
    function paint() {
      counter.set(s.counter);
      const b = BITS[s.tac & 3];
      if (marked !== b) {
        if (marked >= 0) counter.cell(marked).classList.remove('sel');
        counter.cell(b).classList.add('sel');
        marked = b;
      }
      vals.innerHTML =
        `<span>DIV <b>0x${hx(s.counter >> 8)}</b></span><span>TIMA <b>0x${hx(s.tima)}</b></span>` +
        `<span>TMA <b>0x${hx(s.tma)}</b></span><span>fil vers TIMA <b>${+signal()}</b></span>`;
    }
    function loop() {
      if (running) for (let i = 0; i < +speed.value; i++) step();
      paint();
      raf = requestAnimationFrame(loop);
    }
    play.addEventListener('click', () => {
      running = !running;
      play.textContent = running ? 'Pause' : 'Lecture';
    });
    play.textContent = running ? 'Pause' : 'Lecture';
    $(root, '#tm-step').addEventListener('click', () => {
      step();
      paint();
    });
    $(root, '#tm-div').addEventListener('click', () => {
      const before = signal();
      s.counter = 0;
      log.innerHTML = 'Écriture dans DIV : le compteur entier repart de zéro.';
      fall(before, 'écriture dans DIV');
      paint();
    });
    loop();
    return () => cancelAnimationFrame(raf);
  }

  function initPpu(root) {
    const px = $(root, '#tile-px'), pal = $(root, '#tile-pal');
    const run = () => {
      const lo = low.get(), hi = high.get(), p = bgp.get();
      let html = '';
      for (let col = 0; col < 8; col++) {
        const bit = 7 - col;
        const color = (((hi >> bit) & 1) << 1) | ((lo >> bit) & 1);
        const shade = (p >> (color * 2)) & 3;
        html += `<span class="px s${shade}"><b>${color}</b></span>`;
      }
      px.innerHTML = html;
      pal.innerHTML = [0, 1, 2, 3]
        .map(c => `<span class="sw"><span class="px s${(p >> (c * 2)) & 3}"></span>couleur ${c} → teinte ${(p >> (c * 2)) & 3}</span>`)
        .join('');
    };
    const low = buildStrip($(root, '#tile-low'), { value: 0x3c, editable: 0xff, onChange: run });
    const high = buildStrip($(root, '#tile-high'), { value: 0x7e, editable: 0xff, onChange: run });
    const bgp = buildStrip($(root, '#tile-bgp'), {
      value: 0xe4, editable: 0xff, groups: [4, 4, 3, 3, 2, 2, 1, 1], labels: ['c3', 'c3', 'c2', 'c2', 'c1', 'c1', 'c0', 'c0'], onChange: run,
    });
    run();
  }

  function initOam(root) {
    const ROWS = ['03330000', '03003000', '03003000', '03330000', '03030000', '03003000', '03000300', '00000222'];
    const grid = $(root, '#oam-grid'), out = $(root, '#oam-out');
    const run = v => {
      const flipX = v & 0x20, flipY = v & 0x40, obp = v & 0x10 ? 0x1b : 0xe4;
      let html = '';
      for (let r = 0; r < 8; r++) {
        const row = ROWS[flipY ? 7 - r : r];
        for (let c = 0; c < 8; c++) {
          const color = +row[flipX ? 7 - c : c];
          html += color === 0 ? '<span class="px tr"></span>' : `<span class="px s${(obp >> (color * 2)) & 3}"></span>`;
        }
      }
      grid.innerHTML = html;
      out.innerHTML =
        `Attributs <b>0x${hx(v)}</b> : palette ${v & 0x10 ? 'OBP1' : 'OBP0'}` +
        `${flipX ? ', miroir horizontal' : ''}${flipY ? ', miroir vertical' : ''}. ` +
        (v & 0x80 ? 'Le sprite passe <b>derrière</b> les couleurs 1 à 3 du fond, et reste visible sur la couleur 0.' : 'Le sprite passe <b>devant</b> le fond.');
    };
    buildStrip($(root, '#oam-attr'), {
      value: 0, editable: 0xf0, groups: [1, 2, 3, 4], labels: ['derrière', 'miroir Y', 'miroir X', 'palette', '–', '–', '–', '–'], onChange: run,
    });
    run(0);
  }

  function initJoypad(root) {
    const NAMES = ['Droite', 'Gauche', 'Haut', 'Bas', 'A', 'B', 'Select', 'Start'];
    let pressed = 0;
    const out = $(root, '#joy-out'), keys = $(root, '#joy-keys');
    const p1 = buildStrip($(root, '#joy-p1'), {
      value: 0xef, editable: 0x30, groups: [0, 0, 2, 1, 3, 3, 3, 3],
      labels: ['1', '1', 'actions', 'directions', 'Bas · Start', 'Haut · Select', 'Gauche · B', 'Droite · A'],
      onChange: run,
    });
    let select = 0x20;
    function run(v) {
      if (v !== undefined) select = v & 0x30;
      let buttons = 0x0f;
      if (!(select & 0x10)) buttons &= ~(pressed & 0x0f);
      if (!(select & 0x20)) buttons &= ~(pressed >> 4);
      const value = 0xc0 | select | (buttons & 0x0f);
      p1.set(value);
      const rows = [!(select & 0x10) && 'directions', !(select & 0x20) && 'actions'].filter(Boolean);
      out.innerHTML =
        `Le jeu lit <b>0x${hx(value)}</b>. ` +
        (rows.length ? `Rangée choisie : ${rows.join(' et ')}.` : 'Aucune rangée choisie : les 4 bits bas restent à 1, tout semble relâché.');
    }
    NAMES.forEach((name, i) => {
      const b = document.createElement('button');
      b.type = 'button';
      b.className = 'key';
      b.textContent = name;
      b.setAttribute('aria-pressed', 'false');
      b.addEventListener('click', () => {
        pressed ^= 1 << i;
        b.setAttribute('aria-pressed', String(!!(pressed & (1 << i))));
        run();
      });
      keys.appendChild(b);
    });
    run();
  }

  const MAP = [
    [0x0000, 0x3fff, 'ROM, banque 0', 'cartridge.read_rom(addr)', 'rom'],
    [0x4000, 0x7fff, 'ROM, banque choisie par le MBC', 'cartridge.read_rom(addr)', 'mbc'],
    [0x8000, 0x9fff, 'VRAM : tuiles et cartes', 'ppu.read(addr)', 'vram'],
    [0xa000, 0xbfff, 'RAM de la cartouche', 'cartridge.read_ram(addr)', 'sram'],
    [0xc000, 0xdfff, 'WRAM', 'wram[addr - 0xC000]', 'wram'],
    [0xe000, 0xfdff, 'Echo : miroir de C000–DDFF', 'wram[addr - 0xE000]', 'wram'],
    [0xfe00, 0xfe9f, 'OAM : 40 sprites', 'ppu.read(addr)', 'oam'],
    [0xfea0, 0xfeff, 'Rien n’est branché', '0xFF', null],
    [0xff00, 0xff00, 'P1 : les boutons', 'joypad.read()', 'joypad'],
    [0xff01, 0xff02, 'SB, SC : port série', 'sb, sc | 0x7E', 'serial'],
    [0xff04, 0xff07, 'DIV, TIMA, TMA, TAC', 'timer.read(addr)', 'timer'],
    [0xff0f, 0xff0f, 'IF : interruptions en attente', 'if_ | 0xE0', 'interrupts'],
    [0xff10, 0xff3f, 'Son (pas encore émulé)', '0xFF', 'apu'],
    [0xff40, 0xff4b, 'LCDC, STAT, scroll, palettes', 'ppu.read(addr)', 'ppu'],
    [0xff80, 0xfffe, 'HRAM', 'hram[addr - 0xFF80]', 'hram'],
    [0xffff, 0xffff, 'IE : interruptions autorisées', 'ie', 'interrupts'],
  ];

  function initBus(root) {
    const input = $(root, '#bus-addr'), out = $(root, '#bus-out'), table = $(root, '#bus-map');
    table.innerHTML = MAP.map(
      (r, i) =>
        `<button type="button" class="row" data-i="${i}"><code>${hx(r[0], 4)}${r[1] !== r[0] ? '–' + hx(r[1], 4) : ''}</code><span>${r[2]}</span></button>`
    ).join('');
    const rows = [...table.children];
    function run() {
      const addr = hexOf(input);
      if (addr === null || addr > 0xffff) return;
      const i = MAP.findIndex(r => addr >= r[0] && addr <= r[1]);
      rows.forEach((el, k) => el.classList.toggle('on', k === i));
      if (i < 0) {
        out.innerHTML = `<b>0x${hx(addr, 4)}</b> tombe dans le dernier bras du <code>match</code> : un registre pas encore émulé. Lecture : 0xFF. Écriture : ignorée.`;
        return;
      }
      const r = MAP[i];
      out.innerHTML =
        `<b>0x${hx(addr, 4)}</b> → ${r[2]}. Le bus exécute <code>${r[3]}</code>` +
        (r[3].includes('addr -') ? `, soit la case <b>0x${hx(addr - (r[0] === 0xe000 ? 0xe000 : r[0]), 4)}</b> du tableau.` : '.') +
        (r[4] ? ` <button type="button" class="link" data-open="${r[4]}">Ouvrir la fiche</button>` : '');
    }
    rows.forEach(el =>
      el.addEventListener('click', () => {
        input.value = hx(MAP[+el.dataset.i][0], 4);
        run();
      })
    );
    input.addEventListener('input', run);
    run();
  }

  function initWram(root) {
    const input = $(root, '#wram-addr'), out = $(root, '#wram-out');
    const strip = buildStrip($(root, '#wram-strip'), { n: 16, groups: [1, 1, 1, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2] });
    function run() {
      const addr = hexOf(input);
      if (addr === null || addr > 0xffff) return;
      strip.set(addr);
      const cell = addr & 0x1fff;
      if (addr >= 0xc000 && addr <= 0xdfff) {
        out.innerHTML = `Les 3 bits du haut valent <code>110</code> : c'est la WRAM. Les 13 bits du bas donnent la case <b>0x${hx(cell, 4)}</b> (${cell}) sur 8 192.`;
      } else if (addr >= 0xe000 && addr <= 0xfdff) {
        out.innerHTML = `Zone echo : les 13 bits du bas sont les mêmes que pour <b>0x${hx(addr - 0x2000, 4)}</b>. Même case <b>0x${hx(cell, 4)}</b>, même octet.`;
      } else {
        out.innerHTML = `0x${hx(addr, 4)} n'arrive pas à la WRAM : une autre puce répond à cette adresse.`;
      }
    }
    input.addEventListener('input', run);
    run();
  }

  function initVram(root) {
    const ids = ['x', 'ly', 'scx', 'scy'].map(k => $(root, '#vr-' + k));
    const out = $(root, '#vr-out');
    function run() {
      const [x, ly, scx, scy] = ids.map(el => Math.max(0, Math.min(+el.max, +el.value || 0)));
      const bx = (x + scx) & 0xff, by = (ly + scy) & 0xff;
      const col = bx >> 3, row = by >> 3;
      const addr = 0x9800 + row * 32 + col;
      out.innerHTML =
        `<pre class="calc">bx = (${x} + ${scx}) mod 256 = ${bx}\nby = (${ly} + ${scy}) mod 256 = ${by}\ncase = (${col}, ${row})\nadresse = 0x9800 + ${row} × 32 + ${col} = 0x${hx(addr, 4)}</pre>` +
        `<p class="res">L'octet lu en <b>0x${hx(addr, 4)}</b> est un numéro de tuile. Dans cette tuile, la PPU prend la ligne <b>${by & 7}</b> (2 octets) ` +
        `et le pixel <b>${bx & 7}</b>, donc le <b>bit ${7 - (bx & 7)}</b> de chaque octet.</p>`;
    }
    ids.forEach(el => el.addEventListener('input', run));
    run();
  }

  function initMbc(root) {
    const s = { ram: false, low: 1, high: 0, mode: false };
    const addr = $(root, '#mbc-addr'), val = $(root, '#mbc-val'), banks = $(root, '#mbc-banks'), out = $(root, '#mbc-out'), log = $(root, '#mbc-log');
    const strip = buildStrip($(root, '#mbc-strip'), {
      n: 7, groups: [2, 2, 1, 1, 1, 1, 1], labels: ['haut', 'haut', 'bas', 'bas', 'bas', 'bas', 'bas'],
    });
    function write(a, v) {
      if (a <= 0x1fff) {
        s.ram = (v & 0x0f) === 0x0a;
        log.innerHTML = `0000–1FFF : quartet bas ${(v & 0x0f) === 0x0a ? '= 0xA, la RAM s’ouvre' : '≠ 0xA, la RAM se ferme'}.`;
      } else if (a <= 0x3fff) {
        s.low = v & 0x1f || 1;
        log.innerHTML = `2000–3FFF : 5 bits bas = ${v & 0x1f}${(v & 0x1f) === 0 ? ', et 0 devient 1' : ''}.`;
      } else if (a <= 0x5fff) {
        s.high = v & 3;
        log.innerHTML = `4000–5FFF : 2 bits hauts = ${v & 3}.`;
      } else if (a <= 0x7fff) {
        s.mode = (v & 1) !== 0;
        log.innerHTML = `6000–7FFF : mode ${v & 1}.`;
      } else {
        log.innerHTML = 'Hors de la ROM : ce n’est pas une commande pour le MBC.';
      }
      paint();
    }
    function paint() {
      const n = +banks.value;
      const full = (s.high << 5) | s.low;
      strip.set(full);
      const low = s.mode ? s.high << 5 : 0;
      out.innerHTML =
        `<div class="kv"><span>0000–3FFF</span><b>banque ${low % n}</b><i>${s.mode ? `mode 1 : ${s.high} &lt;&lt; 5 = ${low}` : 'mode 0 : toujours 0'}${low >= n ? `, replié modulo ${n}` : ''}</i></div>` +
        `<div class="kv"><span>4000–7FFF</span><b>banque ${full % n}</b><i>${s.high} &lt;&lt; 5 | ${s.low} = ${full}${full >= n ? `, replié modulo ${n}` : ''}</i></div>` +
        `<div class="kv"><span>A000–BFFF</span><b>${s.ram ? 'RAM, banque ' + (s.mode ? s.high : 0) : 'fermée'}</b><i>${s.ram ? (s.mode ? 'mode 1 : les 2 bits hauts' : 'mode 0 : toujours 0') : 'lecture 0xFF, écriture perdue'}</i></div>`;
    }
    $(root, '#mbc-write').addEventListener('click', () => {
      const a = hexOf(addr), v = hexOf(val);
      if (a !== null && v !== null) write(a & 0xffff, v & 0xff);
    });
    root.querySelectorAll('[data-w]').forEach(b =>
      b.addEventListener('click', () => {
        const [a, v] = b.dataset.w.split(':');
        addr.value = a;
        val.value = v;
        write(parseInt(a, 16), parseInt(v, 16));
      })
    );
    banks.addEventListener('change', paint);
    paint();
  }

  // ───────────────────────────── Étapes du scroll ─────────────────────────────

  GB.stages = [
    {
      nav: 'Console',
      kicker: 'RetroEmu · émulateur Game Boy en Rust',
      title: 'Une Game Boy démontée, bit par bit',
      text: 'Fais défiler pour ouvrir la console. Chaque pièce mène au module Rust qui la remplace dans l’émulateur.',
    },
    {
      nav: 'Éclaté',
      kicker: 'Vue éclatée',
      title: 'Six étages de plastique et de silicium',
      text: 'Coque, boutons, écran, carte mère, cartouche, dos. Fais glisser la souris pour tourner autour.',
    },
    {
      nav: 'Face avant',
      kicker: 'Étage 1',
      title: 'Ce que le joueur touche et regarde',
      text: 'Huit boutons lus par un seul registre, et un écran de 160 × 144 pixels en quatre teintes. Clique sur une pièce.',
    },
    {
      nav: 'Carte mère',
      kicker: 'Étage 2',
      title: 'Une grosse puce, deux mémoires, un quartz',
      text: 'Les pistes du circuit forment le bus : tout ce que le CPU sait faire, c’est lire ou écrire à une adresse.',
    },
    {
      nav: 'Puce',
      kicker: 'Étage 3',
      title: 'Dans la puce DMG-CPU',
      text: 'Le processeur, la puce graphique, le timer et les interruptions partagent le même silicium. Dans RetroEmu, ce sont des modules séparés.',
    },
    {
      nav: 'Cartouche',
      kicker: 'Étage 4',
      title: 'Le jeu, sa puce de banques et sa pile',
      text: 'La ROM dépasse souvent les 32 Kio visibles par le CPU. Le MBC choisit le morceau à montrer, la pile garde la sauvegarde.',
    },
    {
      nav: 'Tout',
      kicker: 'Vue d’ensemble',
      title: 'Vingt pièces, vingt fiches',
      text: 'Tout est ouvert. Clique sur un point, ou parcours la liste complète plus bas.',
    },
  ];

  // ───────────────────────────── Fiches ─────────────────────────────

  const C = (GB.components = {});
  const add = c => (C[c.id] = c);

  add({
    id: 'joypad', stage: 2, name: 'Boutons', hw: 'Croix, A, B, Start, Select', src: 'src/joypad.rs', addr: 'FF00 (P1)',
    short: 'Huit boutons, un registre, une logique inversée.',
    html: `
<p class="lede">Les 8 boutons sont câblés en 2 rangées de 4. Le jeu choisit une rangée en écrivant les bits 4-5 de P1, puis lit l'état de ses 4 boutons dans les bits 0-3. Partout, <b>0 veut dire « choisi » ou « enfoncé »</b>.</p>
<div class="widget"><h4>À manipuler : le registre P1</h4>
  <p>Enfonce des boutons, puis clique les bits 4 et 5 comme le ferait le jeu.</p>
  <div class="keys" id="joy-keys"></div>
  <div id="joy-p1"></div>
  <p class="res" id="joy-out"></p>
</div>
<p>Exemple vérifié par les tests : A et Bas enfoncés. Avec la rangée actions (bit 5 à 0), le jeu lit <code>0xDE</code> = <code>1101 1110</code>. Avec la rangée directions, il lit <code>0xE7</code> = <code>1110 0111</code>.</p>
${rs('src/joypad.rs', `
pub fn read(&self) -> u8 {
    let mut buttons = 0x0F; // 1 = relâché
    if self.select & 0x10 == 0 {
        buttons &= !(self.pressed & 0x0F); // rangée directions
    }
    if self.select & 0x20 == 0 {
        buttons &= !(self.pressed >> 4); // rangée actions
    }
    0xC0 | self.select | buttons // les bits 6-7 n'existent pas et se lisent à 1
}`)}
<h3>Du clavier au registre</h3>
<p>SDL2 envoie les touches : flèches, <kbd>X</kbd> pour A, <kbd>Z</kbd> pour B, <kbd>Entrée</kbd> pour Start, <kbd>Retour arrière</kbd> pour Select. <code>press()</code> renvoie <code>true</code> seulement pour un nouvel appui : le bus lève alors l'interruption Joypad (bit 4 de IF).</p>`,
    init: initJoypad,
  });

  add({
    id: 'lcd', stage: 2, name: 'Écran LCD', hw: 'Matrice 160 × 144, 4 teintes', src: 'src/ppu.rs, src/gui.rs', addr: 'FF40 (LCDC), FF47 (BGP)',
    short: '23 040 pixels de 2 bits, rafraîchis 59,7 fois par seconde.',
    html: `
<p class="lede">L'écran ne sait rien faire seul : il affiche ce que la PPU lui envoie, ligne après ligne. 160 × 144 = 23 040 pixels, chacun sur 2 bits.</p>
<div class="shades"><span class="px s0"></span><span class="px s1"></span><span class="px s2"></span><span class="px s3"></span></div>
<p>Dans RetroEmu, l'écran est un tableau : une teinte de 0 (blanc) à 3 (noir) par pixel. La PPU le remplit, la fenêtre SDL2 le recopie dans une texture.</p>
${rs('src/ppu.rs', `
/// L'image en cours de dessin : une teinte (0 à 3) par pixel, ligne après ligne.
framebuffer: [u8; SCREEN_WIDTH * SCREEN_HEIGHT], // 160 × 144`)}
<h3>Le rythme</h3>
<p>Une image dure 70 224 T-cycles, soit 16,74 ms : 59,73 images par seconde. La boucle principale exécute une image, l'affiche, puis dort jusqu'à l'échéance suivante.</p>
<h3>Éteindre l'écran</h3>
<p>Le bit 7 de LCDC est l'interrupteur. À 0, la PPU s'arrête au début de la ligne 0, l'image devient blanche, et la VRAM comme l'OAM sont libres d'accès. Au rallumage, elle repart de là.</p>
<p class="note">Dans la fenêtre : <kbd>L</kbd> ouvre le sélecteur de ROM, <kbd>P</kbd> met en pause, <kbd>Échap</kbd> quitte et écrit la sauvegarde.</p>`,
  });

  add({
    id: 'apu', stage: 2, name: 'Haut-parleur et son', hw: 'APU : 4 canaux sonores', src: 'pas encore de module', addr: 'FF10–FF3F',
    short: 'Le seul étage qui reste muet dans RetroEmu.',
    html: `
<p class="lede">La puce contient quatre canaux sonores : deux ondes carrées, une onde programmable, un générateur de bruit. Ils se pilotent par les registres FF10 à FF3F.</p>
<p><b>RetroEmu ne les émule pas encore.</b> Ces adresses tombent dans le dernier bras du <code>match</code> du bus : une lecture renvoie <code>0xFF</code>, une écriture est ignorée. Les jeux tournent quand même, en silence.</p>
${rs('src/bus.rs', `
_ => 0xFF, // registres pas encore émulés (son...)`)}
<p class="note">C'est l'étape suivante possible, avec le mode couleur (CGB).</p>`,
  });

  add({
    id: 'soc', stage: 3, name: 'Puce DMG-CPU', hw: 'Sharp LR35902, boîtier 80 pattes', src: 'src/gameboy.rs', addr: '',
    short: 'Presque toute la console tient dans ce boîtier.',
    html: `
<p class="lede">Sur la carte, une seule puce fait presque tout : le processeur, la puce graphique, le timer, le son, le contrôle des boutons et du port série, plus trois petites mémoires (OAM, HRAM, boot ROM).</p>
<p><button type="button" class="cta" data-stage="4">Ouvrir la puce</button></p>
<h3>Qui possède qui</h3>
<p>En Rust, deux composants ne peuvent pas modifier le même bus en même temps. RetroEmu range donc les modules en arbre : la console possède le CPU et le bus, et le bus possède tout le reste.</p>
<pre class="tree">GameBoy
├── Cpu            registres, décodage des opcodes
└── Bus            la carte mémoire
    ├── Cartridge  ROM + Box&lt;dyn Mbc&gt;
    ├── Ppu        VRAM, OAM, registres vidéo
    ├── Timer      DIV, TIMA, TMA, TAC
    ├── Joypad     P1
    ├── wram       [u8; 0x2000]
    ├── hram       [u8; 0x7F]
    └── ie, if_    interruptions</pre>
<h3>Un pas de console</h3>
${rs('src/gameboy.rs', `
pub fn step(&mut self) -> Result<u32> {
    let spent = self.cpu.step(&mut self.bus)?; // une instruction
    self.bus.tick(spent);                      // le même temps passe pour les autres puces
    self.frame_cycles += spent;
    Ok(spent)
}`)}
<p>Le CPU donne le rythme : il exécute une instruction, annonce combien de T-cycles elle a pris, et le bus fait avancer la PPU et le timer d'autant.</p>`,
  });

  add({
    id: 'bus', stage: 3, name: 'Bus et carte mémoire', hw: 'Pistes du circuit : 16 fils d’adresse, 8 de données', src: 'src/bus.rs', addr: '0000–FFFF',
    short: 'Une adresse sur 16 bits, et une seule puce qui répond.',
    html: `
<p class="lede">Le CPU ne connaît que des adresses. Il pose 16 bits sur le bus, et la puce qui occupe cette adresse répond avec un octet. Dans RetroEmu, ce câblage est un <code>match</code>.</p>
<div class="widget"><h4>À manipuler : qui répond ?</h4>
  <label class="field">Adresse 0x<input id="bus-addr" class="hex w4" value="C005" maxlength="4" spellcheck="false" autocomplete="off"></label>
  <p class="res" id="bus-out"></p>
  <div class="memmap" id="bus-map"></div>
</div>
${rs('src/bus.rs', `
pub fn read8(&self, addr: u16) -> u8 {
    match addr {
        0x0000..=0x7FFF => self.cartridge.read_rom(addr),
        0x8000..=0x9FFF => self.ppu.read(addr),
        0xA000..=0xBFFF => self.cartridge.read_ram(addr),
        0xC000..=0xDFFF => self.wram[(addr - 0xC000) as usize],
        0xE000..=0xFDFF => self.wram[(addr - 0xE000) as usize], // echo
        // ...
        0xFF0F => self.if_ | 0xE0, // les bits 5-7 n'existent pas et se lisent à 1
        _ => 0xFF,
    }
}`)}
<h3>Les bits qui n'existent pas</h3>
<p>Un registre n'a pas toujours 8 bascules. Les bits absents se lisent à 1, et le bus les ajoute avec un OU :</p>
<ul>
  <li><code>IF | 0xE0</code> : 5 sources d'interruption, 3 bits fantômes.</li>
  <li><code>SC | 0x7E</code> : seuls les bits 7 et 0 existent.</li>
  <li><code>STAT | 0x80</code>, <code>TAC | 0xF8</code>, <code>P1 | 0xC0</code> : même principe.</li>
</ul>
<h3>Deux octets, petit bout devant</h3>
<p>Une valeur 16 bits occupe 2 cases, l'octet de poids faible en premier. <code>0x0150</code> est rangé <code>50 01</code>.</p>
${rs('src/bus.rs', `
pub fn read16(&self, addr: u16) -> u16 {
    let low = self.read8(addr) as u16;
    let high = self.read8(addr.wrapping_add(1)) as u16;
    (high << 8) | low
}`)}
<h3>Le bus distribue aussi le temps</h3>
<p>Après chaque instruction, <code>bus.tick(cycles)</code> fait avancer la PPU et le timer, et range leurs demandes d'interruption dans IF.</p>`,
    init: initBus,
  });

  add({
    id: 'wram', stage: 3, name: 'WRAM', hw: 'Sharp LH5264, SRAM 8 Kio', src: 'src/bus.rs', addr: 'C000–DFFF, echo E000–FDFF',
    short: '8 192 octets de mémoire de travail pour le jeu.',
    html: `
<p class="lede">La mémoire de travail du jeu : variables, positions, pile parfois. 8 192 cases de 8 bits, soit 65 536 bascules. Elle s'efface quand on éteint.</p>
${rs('src/bus.rs', `
/// 8 KiB de RAM de travail (0xC000–0xDFFF).
wram: [u8; 0x2000],`)}
<div class="widget"><h4>À manipuler : découper une adresse</h4>
  <label class="field">Adresse 0x<input id="wram-addr" class="hex w4" value="C005" maxlength="4" spellcheck="false" autocomplete="off"></label>
  <div id="wram-strip"></div>
  <p class="res" id="wram-out"></p>
</div>
<h3>L'echo RAM</h3>
<p>8 192 = 2<sup>13</sup> : la puce n'a besoin que de 13 fils d'adresse. Entre <code>C005</code> et <code>E005</code>, seul le bit 13 change, et il n'arrive pas jusqu'à la puce. Les deux adresses désignent donc le même octet. Le bus reproduit ce miroir avec deux bras qui lisent le même tableau.</p>
<p class="note">Essaie <code>E005</code> dans le widget : la case ne bouge pas.</p>`,
    init: initWram,
  });

  add({
    id: 'vram', stage: 3, name: 'VRAM', hw: 'Sharp LH5264, SRAM 8 Kio', src: 'src/ppu.rs', addr: '8000–9FFF',
    short: 'Les dessins des tuiles et le plan du décor.',
    html: `
<p class="lede">La mémoire de la puce graphique. Le jeu y écrit deux choses : les dessins des tuiles, et les cartes qui disent quelle tuile va dans quelle case.</p>
<div class="bar"><span style="flex:6" class="b1"><b>8000–97FF</b>384 tuiles × 16 octets</span><span style="flex:1.4" class="b2"><b>9800</b>carte 0</span><span style="flex:1.4" class="b3"><b>9C00</b>carte 1</span></div>
<ul>
  <li><b>Une tuile</b> : 8 × 8 pixels, 2 bits par pixel, donc 2 octets par ligne et 16 octets en tout.</li>
  <li><b>Une carte</b> : 32 × 32 cases, 1 octet par case (un numéro de tuile). Le décor complet fait 256 × 256 pixels.</li>
</ul>
<p>L'écran n'en montre qu'une fenêtre de 160 × 144, placée par SCX et SCY. Les calculs se font sur 8 bits : en dépassant 255, on revient à 0 et le décor boucle.</p>
<div class="widget"><h4>À manipuler : d'un pixel de l'écran à un bit de la VRAM</h4>
  <div class="fields">
    <label class="field">x <input id="vr-x" type="number" min="0" max="159" value="10"></label>
    <label class="field">LY <input id="vr-ly" type="number" min="0" max="143" value="20"></label>
    <label class="field">SCX <input id="vr-scx" type="number" min="0" max="255" value="100"></label>
    <label class="field">SCY <input id="vr-scy" type="number" min="0" max="255" value="6"></label>
  </div>
  <div id="vr-out"></div>
</div>
${rs('src/ppu.rs', `
fn map_color(&self, map: u16, x: u8, y: u8) -> u8 {
    // Quelle tuile ? La carte fait 32 cases de large.
    let tile = self.vram_read(map + (y / 8) as u16 * 32 + (x / 8) as u16);
    // Quelle ligne, quel pixel de la tuile ?
    self.tile_pixel(tile, y % 8, x % 8)
}`)}
<h3>Deux façons de numéroter les tuiles</h3>
<p>Le bit 4 de LCDC choisit. À 1, la tuile <i>n</i> est en <code>0x8000 + n × 16</code>. À 0, le numéro est signé et tourne autour de <code>0x9000</code> : la tuile <code>0x80</code> (−128) est en <code>0x8800</code>. Les sprites utilisent toujours le premier mode.</p>
<h3>Verrouillée pendant le dessin</h3>
<p>En mode 3, la PPU lit la VRAM en continu. Le CPU, lui, lit <code>0xFF</code>, et ses écritures sont perdues.</p>`,
    init: initVram,
  });

  add({
    id: 'quartz', stage: 3, name: 'Quartz', hw: 'Oscillateur 4,194304 MHz', src: 'src/gameboy.rs, src/main.rs', addr: '',
    short: '4 194 304 battements par seconde : l’unité de temps de tout l’émulateur.',
    html: `
<p class="lede">Le quartz bat 4 194 304 fois par seconde, soit 2<sup>22</sup>. Chaque battement est un <b>T-cycle</b>. Tout le reste de la console compte dans cette unité.</p>
<div class="kvs">
  <div class="kv"><span>1 M-cycle</span><b>4 T-cycles</b><i>le pas du CPU : un accès mémoire</i></div>
  <div class="kv"><span>1 ligne d'écran</span><b>456 T-cycles</b><i>456 « dots » pour la PPU</i></div>
  <div class="kv"><span>1 image</span><b>70 224 T-cycles</b><i>154 lignes × 456</i></div>
  <div class="kv"><span>Cadence</span><b>59,73 Hz</b><i>4 194 304 ÷ 70 224</i></div>
  <div class="kv"><span>DIV</span><b>16 384 Hz</b><i>+1 tous les 256 T-cycles</i></div>
</div>
<h3>Dans l'émulateur, pas de quartz</h3>
<p>Le temps est compté, pas mesuré. Chaque instruction renvoie son coût en T-cycles, et la console en exécute jusqu'à remplir une image.</p>
${rs('src/gameboy.rs', `
pub const CYCLES_PER_FRAME: u32 = 70_224;

while self.frame_cycles < CYCLES_PER_FRAME {
    self.step()?;
}
self.frame_cycles -= CYCLES_PER_FRAME; // le surplus compte pour l'image suivante`)}
<p>La dernière instruction déborde souvent de quelques cycles. Les garder pour l'image suivante évite que l'horloge dérive.</p>
<h3>Recoller au temps réel</h3>
<p>Un PC exécute une image en bien moins de 16,74 ms. La boucle dort donc jusqu'à une échéance qui avance toujours de la même durée : si un <code>sleep</code> dure trop longtemps, le retard est rattrapé à l'image suivante au lieu de s'accumuler.</p>
${rs('src/main.rs', `
const FRAME_DURATION: Duration =
    Duration::from_nanos(CYCLES_PER_FRAME as u64 * 1_000_000_000 / 4_194_304);`)}`,
  });

  add({
    id: 'serial', stage: 3, name: 'Port série', hw: 'Prise du câble Link', src: 'src/bus.rs', addr: 'FF01 (SB), FF02 (SC)',
    short: 'Un octet à la fois, et la sortie des ROMs de test.',
    html: `
<p class="lede">Le câble Link échange un octet, bit par bit, entre deux consoles. SB contient l'octet, SC lance le transfert.</p>
${st(0x81, ['départ', '–', '–', '–', '–', '–', '–', 'horloge'], [1, 0, 0, 0, 0, 0, 0, 2])}
<ul>
  <li><b>Bit 7</b> : le jeu le met à 1 pour demander un transfert.</li>
  <li><b>Bit 0</b> : 1 si cette console donne le rythme.</li>
  <li>Les bits 1 à 6 n'existent pas : SC se lit <code>sc | 0x7E</code>.</li>
</ul>
<h3>Sans câble</h3>
<p>RetroEmu n'a personne en face. Quand SC vaut <code>0x81</code>, l'octet de SB part dans un tampon, SB reçoit <code>0xFF</code> (un câble vide), le bit 7 retombe et l'interruption Série est demandée.</p>
${rs('src/bus.rs', `
fn write_sc(&mut self, value: u8) {
    self.sc = value & 0x81;
    if self.sc == 0x81 {
        self.serial_output.push(self.sb);
        self.sb = 0xFF;
        self.sc &= 0x01; // bit 7 à 0 : « transfert terminé »
        self.request_interrupt(INT_SERIAL);
    }
}`)}
<h3>À quoi ça sert sans second joueur</h3>
<p>Les ROMs de test Blargg écrivent leur verdict, lettre par lettre, sur ce port. Le tampon est affiché dans le terminal : on peut valider le CPU avant même d'avoir un écran.</p>
<p class="note">Simplification : sur la console, un transfert prend 4 096 T-cycles. Ici il est instantané.</p>`,
  });

  add({
    id: 'cpu', stage: 4, name: 'CPU SM83', hw: 'Cœur Sharp SM83, 8 bits', src: 'src/cpu.rs, src/cpu/*.rs', addr: '',
    short: 'Lire un octet, le décoder, l’exécuter : 501 instructions.',
    html: `
<p class="lede">Un processeur 8 bits, cousin du Z80. Il répète trois gestes : lire l'octet pointé par PC, le décoder, l'exécuter. <code>step()</code> fait un tour et renvoie le nombre de T-cycles consommés.</p>
<h3>Les registres</h3>
<div class="regs">
  <span><i>A</i>01</span><span><i>F</i>B0</span><span><i>B</i>00</span><span><i>C</i>13</span>
  <span><i>D</i>00</span><span><i>E</i>D8</span><span><i>H</i>01</span><span><i>L</i>4D</span>
  <span class="wide"><i>SP</i>FFFE</span><span class="wide"><i>PC</i>0100</span>
</div>
<p>Huit registres de 8 bits, qui s'assemblent en paires de 16 bits (AF, BC, DE, HL), plus le pointeur de pile et le compteur de programme. Les valeurs ci-dessus sont celles de <code>Cpu::new()</code> : l'état laissé par la boot ROM.</p>
<h3>F, le registre des drapeaux</h3>
${st(0xb0, ['Z', 'N', 'H', 'C', '0', '0', '0', '0'], [1, 2, 3, 4])}
<ul>
  <li><b>Z</b> : le résultat vaut zéro.</li>
  <li><b>N</b> : la dernière opération était une soustraction.</li>
  <li><b>H</b> : retenue du bit 3 vers le bit 4 (half-carry).</li>
  <li><b>C</b> : retenue qui sort du bit 7.</li>
  <li>Les 4 bits bas n'existent pas : <code>set_af</code> applique <code>&amp; 0xF0</code>.</li>
</ul>
<div class="widget"><h4>À manipuler : l'ALU et ses drapeaux</h4>
  <div class="fields">
    <label class="field">A 0x<input id="alu-a" class="hex" value="3A" maxlength="2" spellcheck="false" autocomplete="off"></label>
    <label class="field">opération <select id="alu-op"><option value="add">ADD</option><option value="sub">SUB</option></select></label>
    <label class="field">n 0x<input id="alu-b" class="hex" value="C6" maxlength="2" spellcheck="false" autocomplete="off"></label>
  </div>
  <div id="alu-out"></div>
  <div id="alu-flags"></div>
</div>
${rs('src/cpu/alu.rs', `
let sum = a as u16 + value as u16 + carry as u16;
let half = (a & 0x0F) + (value & 0x0F) + carry > 0x0F;
(sum as u8, half, sum > 0xFF)`)}
<h3>Décoder par motifs de bits</h3>
<p>Un opcode est un octet : 256 cases. La moitié suit un motif régulier, <code>xx yyy zzz</code>, où y et z désignent un registre avec le même code partout : 0 B, 1 C, 2 D, 3 E, 4 H, 5 L, 6 (HL), 7 A.</p>
<div class="widget"><h4>À manipuler : de 0x40 à 0xBF</h4>
  <input id="op-range" type="range" min="64" max="191" value="120" aria-label="Opcode">
  <div id="op-strip"></div>
  <p class="res" id="op-out"></p>
</div>
${rs('src/cpu.rs', `
let cycles = match opcode {
    0x00 => 4, // NOP
    0x76 => self.halt(bus),
    0x40..=0x7F => self.ld_r_r(bus, opcode),
    0x80..=0xBF => self.alu_r(bus, opcode),
    // ...
};`)}
<p>Deux fonctions couvrent ainsi 127 opcodes. Le code 6, (HL), passe par le bus et coûte 4 T-cycles de plus.</p>
<h3>L'inventaire</h3>
<p>245 opcodes valides, plus 256 instructions derrière le préfixe <code>CB</code> (rotations, décalages, tests de bits). Le <code>match</code> n'a pas de bras <code>_</code> : le compilateur vérifie que les 256 valeurs sont traitées. Les 11 octets sans instruction renvoient une erreur.</p>`,
    init: initCpu,
  });

  add({
    id: 'interrupts', stage: 4, name: 'Interruptions', hw: 'Registres IE et IF, bascule IME', src: 'src/cpu/interrupts.rs, src/bus.rs', addr: 'FFFF (IE), FF0F (IF)',
    short: 'Cinq sonnettes, deux registres, un interrupteur général.',
    html: `
<p class="lede">Une puce qui a besoin du CPU lève un bit dans IF : elle sonne. Le jeu choisit dans IE quelles sonnettes il écoute. IME est l'interrupteur général, piloté par les instructions DI et EI.</p>
<div class="widget"><h4>À manipuler : qui passe en premier ?</h4>
  <p class="cap">IE : autorisées par le jeu</p><div id="int-ie"></div>
  <p class="cap">IF : en attente</p><div id="int-if"></div>
  <label class="check"><input type="checkbox" id="int-ime" checked> IME = 1</label>
  <p class="res" id="int-out"></p>
</div>
${rs('src/cpu/interrupts.rs', `
pub(super) fn service_interrupt(&mut self, bus: &mut Bus) -> u32 {
    let bit = bus.pending_interrupts().trailing_zeros() as u16; // le bit 1 le plus bas
    bus.clear_interrupt(1 << bit);
    self.ime = false;
    self.push16(bus, self.pc);
    self.pc = 0x40 + 8 * bit;
    20
}`)}
<p>La priorité ne demande aucun tableau : <code>trailing_zeros()</code> donne le rang du premier bit à 1, donc la source la plus prioritaire. Le même rang donne l'adresse du gestionnaire.</p>
<h3>Trois détails qui comptent</h3>
<ul>
  <li><b>Le retard d'EI.</b> EI ne rallume IME qu'après l'instruction suivante. C'est ce qui permet d'écrire <code>EI</code> puis <code>RET</code> sans être coupé entre les deux.</li>
  <li><b>HALT.</b> Le CPU dort jusqu'à ce que IE &amp; IF ≠ 0, même si IME vaut 0. Pendant ce temps, <code>step()</code> renvoie 4 cycles pour que les autres puces avancent.</li>
  <li><b>Le HALT bug.</b> HALT avec IME = 0 et une interruption déjà en attente : le CPU ne dort pas, et l'octet suivant est lu deux fois. RetroEmu le reproduit.</li>
</ul>`,
    init: initInterrupts,
  });

  add({
    id: 'timer', stage: 4, name: 'Timer', hw: 'Compteur 16 bits et registres DIV, TIMA, TMA, TAC', src: 'src/timer.rs', addr: 'FF04–FF07',
    short: 'Un compteur de 16 bits, et un fil branché sur l’un de ses bits.',
    html: `
<p class="lede">Un seul compteur de 16 bits avance à chaque T-cycle. <b>DIV</b> en est l'octet haut. <b>TIMA</b> est branché sur un de ses bits, choisi par TAC, et avance quand ce bit retombe de 1 à 0.</p>
<div class="widget"><h4>À manipuler : le compteur en marche</h4>
  <p class="cap">Compteur interne. En couleur : DIV. Encadré : le bit suivi par TIMA.</p>
  <div id="tm-counter"></div>
  <div class="vals" id="tm-vals"></div>
  <p class="cap">TAC</p><div id="tm-tac"></div>
  <div class="fields">
    <button type="button" id="tm-play">Pause</button>
    <button type="button" id="tm-step">+4 cycles</button>
    <button type="button" id="tm-div">Écrire dans DIV</button>
    <label class="field">vitesse <select id="tm-speed"><option value="1">× 1</option><option value="8">× 8</option><option value="64">× 64</option></select></label>
  </div>
  <p class="res" id="tm-log">Change la vitesse dans TAC, ou coupe le bit « marche » pendant que le fil vaut 1.</p>
</div>
<div class="kvs">
  <div class="kv"><span>TAC = 00</span><b>bit 9</b><i>4 096 Hz, tous les 1 024 T-cycles</i></div>
  <div class="kv"><span>TAC = 01</span><b>bit 3</b><i>262 144 Hz, tous les 16</i></div>
  <div class="kv"><span>TAC = 10</span><b>bit 5</b><i>65 536 Hz, tous les 64</i></div>
  <div class="kv"><span>TAC = 11</span><b>bit 7</b><i>16 384 Hz, tous les 256</i></div>
</div>
${rs('src/timer.rs', `
/// Le fil qui arrive à TIMA : le bit du compteur choisi par TAC, ET l'interrupteur.
fn signal(&self) -> bool {
    let bit = match self.tac & 0x03 {
        0 => 9,
        1 => 3,
        2 => 5,
        3 => 7,
        _ => unreachable!(),
    };
    self.tac & TAC_ENABLE != 0 && (self.counter >> bit) & 1 == 1
}

/// Fait avancer TIMA si le signal vient de passer de 1 à 0.
fn detect_fall(&mut self, before: bool) {
    if before && !self.signal() {
        self.increment_tima();
    }
}`)}
<h3>Pourquoi copier le circuit</h3>
<p>Ce modèle reproduit les bizarreries du matériel sans cas particulier. Écrire dans DIV remet le compteur à zéro : si le bit suivi valait 1, il retombe, et TIMA avance. Éteindre le timer au mauvais moment fait la même chose. Le bouton « Écrire dans DIV » le montre.</p>
<h3>Le débordement</h3>
<p>Quand TIMA dépasse <code>0xFF</code>, il repart de la valeur de TMA et le timer demande l'interruption Timer. Avec TMA, le jeu règle la période.</p>
<p class="note">Simplification : sur la console, TIMA vaut 0x00 pendant 4 T-cycles avant d'être rechargé. Ici le rechargement est immédiat.</p>`,
    init: initTimer,
  });

  add({
    id: 'ppu', stage: 4, name: 'PPU', hw: 'Picture Processing Unit', src: 'src/ppu.rs', addr: 'FF40–FF4B',
    short: 'Elle transforme des octets en image, une ligne à la fois.',
    html: `
<p class="lede">La puce graphique lit la VRAM et l'OAM, et fabrique l'image ligne par ligne. Son horloge est le <b>dot</b>, égal à un T-cycle.</p>
<h3>Une ligne : 456 dots, trois modes</h3>
<div class="bar"><span style="flex:80" class="b2"><b>Mode 2</b>80 · scan OAM</span><span style="flex:172" class="b1"><b>Mode 3</b>172 · dessin</span><span style="flex:204" class="b3"><b>Mode 0</b>204 · HBlank</span></div>
<p>144 lignes visibles se suivent ainsi, puis 10 lignes de repos : le <b>mode 1</b>, VBlank. 154 × 456 = 70 224 dots par image. Le jeu profite de HBlank et de VBlank pour modifier la mémoire vidéo.</p>
<p>Le mode n'est pas stocké : il se déduit de LY et du nombre de dots écoulés dans la ligne.</p>
<h3>Une ligne de tuile : 2 octets pour 8 pixels</h3>
<p>Le premier octet porte les bits bas des 8 pixels, le second les bits hauts. Le pixel 0, à gauche, est le bit 7.</p>
<div class="widget"><h4>À manipuler : décoder une ligne, puis choisir les teintes</h4>
  <p class="cap">Octet 1 : bits bas</p><div id="tile-low"></div>
  <p class="cap">Octet 2 : bits hauts</p><div id="tile-high"></div>
  <p class="cap">Les 8 pixels et leur numéro de couleur</p><div class="pxrow" id="tile-px"></div>
  <p class="cap">BGP : 2 bits par couleur, la couleur 0 à droite</p><div id="tile-bgp"></div>
  <div class="swatches" id="tile-pal"></div>
</div>
${rs('src/ppu.rs', `
fn row_pixel(&self, addr: u16, col: u8) -> u8 {
    let low = self.vram_read(addr);      // les bits bas des 8 pixels
    let high = self.vram_read(addr + 1); // les bits hauts
    let bit = 7 - col; // le pixel 0 est le bit 7
    ((high >> bit) & 1) << 1 | ((low >> bit) & 1)
}

fn shade(palette: u8, color: u8) -> u8 {
    (palette >> (color * 2)) & 0x03
}`)}
<h3>LCDC : huit interrupteurs</h3>
${st(0x91, ['écran', 'carte window', 'window', 'tuiles', 'carte fond', 'taille sprites', 'sprites', 'fond'], [1, 3, 3, 2, 2, 4, 4, 2])}
<ul class="tight">
  <li><b>7</b> écran allumé · <b>6</b> carte de la window (9800 ou 9C00) · <b>5</b> window affichée</li>
  <li><b>4</b> numérotation des tuiles (8800 ou 8000) · <b>3</b> carte du fond (9800 ou 9C00)</li>
  <li><b>2</b> sprites 8 × 8 ou 8 × 16 · <b>1</b> sprites affichés · <b>0</b> fond et window affichés</li>
</ul>
<h3>STAT : l'état, et quatre demandes d'interruption</h3>
${st(0x85, ['1', 'LYC', 'mode 2', 'mode 1', 'mode 0', 'LY = LYC', 'mode', 'mode'], [0, 1, 1, 1, 1, 3, 2, 2])}
<p>Les bits 0 à 2 sont calculés par la PPU. Le jeu n'écrit que les bits 3 à 6 (<code>value &amp; 0x78</code>) : « préviens-moi en HBlank », « quand LY = LYC »...</p>
<p>Ces quatre sources sont reliées par un OU à un seul fil. L'interruption STAT part uniquement quand ce fil <b>monte</b> de 0 à 1. Deux sources qui s'enchaînent sans que le fil retombe ne sonnent donc qu'une fois.</p>
<h3>Un pixel, trois questions</h3>
<p>RetroEmu dessine la ligne entière à la fin du mode 3. Pour chacun des 160 pixels :</p>
<ol>
  <li>Quel numéro de couleur donne le fond, ou la window si elle couvre ce pixel ?</li>
  <li>Quel est le premier sprite, par priorité, qui n'est pas transparent ici ?</li>
  <li>Le sprite gagne, sauf s'il est marqué « derrière » et que le fond a une couleur 1 à 3.</li>
</ol>
<h3>La window</h3>
<p>Un second calque, sans défilement, pour les barres d'état. Elle commence à la colonne WX − 7, à partir de la ligne WY. Elle a son propre compteur de lignes, qui n'avance que sur les lignes où elle est dessinée.</p>
<p class="note">Simplifications : le mode 3 dure toujours 172 dots, alors qu'il s'allonge sur la console selon les sprites et le scroll. Le rendu se fait par ligne entière, pas pixel par pixel avec une FIFO.</p>`,
    init: initPpu,
  });

  add({
    id: 'oam', stage: 4, name: 'OAM et sprites', hw: 'Object Attribute Memory, 160 octets', src: 'src/ppu.rs, src/bus.rs', addr: 'FE00–FE9F, FF46 (DMA)',
    short: '40 fiches de 4 octets : une par sprite.',
    html: `
<p class="lede">Les sprites sont les objets qui bougent par-dessus le décor. L'OAM contient 40 fiches de 4 octets.</p>
<div class="kvs">
  <div class="kv"><span>Octet 0</span><b>Y + 16</b><i>16 = collé en haut de l'écran</i></div>
  <div class="kv"><span>Octet 1</span><b>X + 8</b><i>8 = collé à gauche</i></div>
  <div class="kv"><span>Octet 2</span><b>tuile</b><i>toujours en mode 8000</i></div>
  <div class="kv"><span>Octet 3</span><b>attributs</b><i>4 bits utiles</i></div>
</div>
<p>Les décalages de 16 et de 8 permettent de cacher un sprite à moitié, ou en entier, au-dessus et à gauche de l'écran sans nombre négatif.</p>
<div class="widget"><h4>À manipuler : l'octet d'attributs</h4>
  <div class="oam"><div class="sprite" id="oam-grid"></div><div id="oam-attr"></div></div>
  <p class="res" id="oam-out"></p>
</div>
<h3>Dix par ligne</h3>
<p>En mode 2, la PPU parcourt les 40 fiches et garde les 10 premières qui croisent la ligne LY. Seul Y compte : un sprite caché à gauche prend quand même une place. En 8 × 16, le bit 0 du numéro de tuile est ignoré.</p>
<h3>Qui passe devant</h3>
<p>Le plus petit X gagne. À X égal, c'est l'ordre dans l'OAM. Le tri de Rust est stable, donc un simple <code>sort_by_key(|s| s.x)</code> donne les deux règles d'un coup.</p>
<h3>Le DMA</h3>
<p>Écrire une valeur <i>v</i> dans FF46 copie 160 octets depuis <code>v × 0x100</code> vers l'OAM. Les jeux préparent leurs sprites en WRAM, puis les envoient en une fois pendant VBlank.</p>
${rs('src/bus.rs', `
fn dma(&mut self, value: u8) {
    let source = (value as u16) << 8;
    for i in 0..0xA0 {
        let byte = self.read8(source + i);
        self.ppu.write_oam_dma(i as usize, byte);
    }
}`)}
<p class="note">Simplification : sur la console, la copie prend 640 T-cycles. Ici elle est instantanée. L'OAM est verrouillée pour le CPU pendant les modes 2 et 3.</p>`,
    init: initOam,
  });

  add({
    id: 'hram', stage: 4, name: 'HRAM', hw: '127 octets de RAM dans la puce', src: 'src/bus.rs', addr: 'FF80–FFFE',
    short: 'Une petite mémoire toujours accessible.',
    html: `
<p class="lede">127 octets logés dans la puce elle-même, tout en haut de la carte mémoire, juste sous IE.</p>
${rs('src/bus.rs', `
/// 127 octets de RAM rapide (0xFF80–0xFFFE).
hram: [u8; 0x7F],`)}
<h3>À quoi elle sert</h3>
<ul>
  <li><b>Pendant un DMA</b>, sur la console, le CPU ne peut lire que la HRAM. Les jeux y copient donc la petite routine qui lance le DMA et attend sa fin.</li>
  <li><b>La pile</b> démarre juste là : après la boot ROM, SP vaut <code>0xFFFE</code>.</li>
  <li><b>Les adresses courtes.</b> Les instructions <code>LDH</code> (opcodes E0 et F0) n'encodent que l'octet bas : l'adresse est <code>0xFF00 + n</code>. Elles atteignent les registres et la HRAM en 2 octets au lieu de 3.</li>
</ul>`,
  });

  add({
    id: 'bootrom', stage: 4, name: 'Boot ROM', hw: '256 octets gravés dans la puce', src: 'src/cpu.rs', addr: '0000–00FF au démarrage',
    short: 'Le programme de démarrage, que RetroEmu saute.',
    html: `
<p class="lede">À l'allumage, la console exécute 256 octets gravés dans la puce : le logo qui descend, le « ding », et la vérification du header de la cartouche. Puis ce programme se débranche et saute en <code>0x0100</code>, au début du jeu.</p>
<h3>RetroEmu commence après</h3>
<p>La boot ROM n'est pas émulée. <code>Cpu::new()</code> place directement les registres dans l'état qu'elle laisse derrière elle sur une Game Boy d'origine :</p>
${rs('src/cpu.rs', `
Cpu {
    a: 0x01, f: 0xB0,
    b: 0x00, c: 0x13,
    d: 0x00, e: 0xD8,
    h: 0x01, l: 0x4D,
    sp: 0xFFFE,
    pc: 0x0100,
    // ...
}`)}
<p>Certains jeux lisent ces valeurs : A = <code>0x01</code> signale une Game Boy classique.</p>
<h3>Le contrôle du header</h3>
<p>La boot ROM refuse de lancer une cartouche dont la somme de contrôle est fausse. RetroEmu fait le même calcul au chargement et affiche un avertissement, sans bloquer.</p>`,
  });

  add({
    id: 'rom', stage: 5, name: 'ROM du jeu', hw: 'Mémoire morte, de 32 Kio à 8 Mio', src: 'src/cartridge.rs', addr: '0000–7FFF',
    short: 'Le programme, et un header qui décrit la cartouche.',
    html: `
<p class="lede">Le fichier <code>.gb</code> est la copie de cette puce. Avant de l'exécuter, RetroEmu lit son header : 80 octets qui décrivent la cartouche.</p>
<div class="kvs">
  <div class="kv"><span>0100–0103</span><b>point d'entrée</b><i>presque toujours NOP, JP 0x0150</i></div>
  <div class="kv"><span>0134–0143</span><b>titre</b><i>ASCII, complété par des zéros</i></div>
  <div class="kv"><span>0143</span><b>drapeau CGB</b><i>bit 7 à 1 : le jeu connaît la couleur</i></div>
  <div class="kv"><span>0147</span><b>type</b><i>quel MBC, avec RAM, avec pile</i></div>
  <div class="kv"><span>0148</span><b>taille ROM</b><i>32 Kio &lt;&lt; code</i></div>
  <div class="kv"><span>0149</span><b>taille RAM</b><i>02 : 8 Kio · 03 : 32 Kio · 04 : 128 Kio · 05 : 64 Kio</i></div>
  <div class="kv"><span>014D</span><b>somme de contrôle</b><i>sur 0134–014C</i></div>
</div>
<h3>La somme de contrôle</h3>
${rs('src/cartridge.rs', `
let mut computed_checksum: u8 = 0;
for &b in &rom[0x0134..=0x014C] {
    computed_checksum = computed_checksum.wrapping_sub(b).wrapping_sub(1);
}`)}
<p>On part de 0 et, pour chaque octet, on retire l'octet puis 1, le tout sur 8 bits. Le résultat doit être égal à l'octet en <code>0x014D</code>.</p>
<h3>L'octet 0147 choisit le contrôleur</h3>
<div class="kvs">
  <div class="kv"><span>00</span><b>ROM seule</b><i>32 Kio, pas de banques</i></div>
  <div class="kv"><span>01–03</span><b>MBC1</b><i>+RAM, +pile</i></div>
  <div class="kv"><span>05–06</span><b>MBC2</b><i>RAM de 4 bits dans la puce</i></div>
  <div class="kv"><span>19–1E</span><b>MBC5</b><i>+RAM, +pile, vibreur ignoré</i></div>
</div>
<p>Un type inconnu arrête le chargement avec un message qui nomme le jeu et son contrôleur.</p>
<h3>Écrire dans une ROM ?</h3>
<p>Une écriture entre 0000 et 7FFF ne modifie jamais le jeu. Elle arrive à la puce voisine : <button type="button" class="link" data-open="mbc">le MBC</button>.</p>`,
  });

  add({
    id: 'mbc', stage: 5, name: 'Contrôleur de banques', hw: 'Puce MBC1, MBC2 ou MBC5', src: 'src/mbc.rs', addr: 'commandes en 0000–7FFF',
    short: 'Il choisit quel morceau de ROM le CPU peut voir.',
    html: `
<p class="lede">Le CPU ne voit que 32 Kio de ROM. Les jeux plus gros sont découpés en banques de 16 Kio : la banque 0 reste en 0000–3FFF, et le MBC choisit celle qui apparaît en 4000–7FFF. Le jeu le commande en « écrivant dans la ROM ».</p>
<div class="widget"><h4>À manipuler : un MBC1</h4>
  <div class="fields">
    <label class="field">écrire 0x<input id="mbc-val" class="hex" value="21" maxlength="2" spellcheck="false" autocomplete="off"></label>
    <label class="field">en 0x<input id="mbc-addr" class="hex w4" value="2100" maxlength="4" spellcheck="false" autocomplete="off"></label>
    <button type="button" id="mbc-write">Écrire</button>
    <label class="field">ROM <select id="mbc-banks"><option value="4">64 Kio · 4 banques</option><option value="32">512 Kio · 32</option><option value="128" selected>2 Mio · 128</option></select></label>
  </div>
  <div class="fields presets">
    <button type="button" data-w="0000:0A">ouvrir la RAM</button>
    <button type="button" data-w="2000:00">banque 0</button>
    <button type="button" data-w="2000:1F">5 bits bas à 31</button>
    <button type="button" data-w="4000:02">2 bits hauts à 2</button>
    <button type="button" data-w="6000:01">mode 1</button>
    <button type="button" data-w="6000:00">mode 0</button>
  </div>
  <p class="res" id="mbc-log">Les quatre plages d'adresses sont quatre registres.</p>
  <p class="cap">Numéro de banque : 2 bits hauts collés devant 5 bits bas</p>
  <div id="mbc-strip"></div>
  <div class="kvs" id="mbc-out"></div>
</div>
${rs('src/mbc.rs', `
fn write_rom(&mut self, addr: u16, value: u8) {
    match addr {
        0x0000..=0x1FFF => self.ram_enabled = value & 0x0F == 0x0A,
        0x2000..=0x3FFF => {
            // 0 devient 1, car la banque 0 est déjà en 0x0000–0x3FFF.
            self.bank_low = match value & 0x1F {
                0 => 1,
                bank => bank,
            };
        }
        0x4000..=0x5FFF => self.bank_high = value & 0x03,
        _ => self.advanced_mode = value & 0x01 != 0,
    }
}`)}
<h3>Les pièges du MBC1</h3>
<ul>
  <li><b>0 devient 1</b>, mais seulement sur les 5 bits bas. Les banques 0x20, 0x40 et 0x60 sont donc inaccessibles en 4000–7FFF : 125 banques utiles sur 128.</li>
  <li><b>Les bits en trop sont ignorés.</b> Sur une ROM de 4 banques, demander la banque 5 donne la banque 1 : le numéro est replié modulo le nombre de banques.</li>
  <li><b>Le mode 1</b> applique aussi les 2 bits hauts à la zone 0000–3FFF et à la banque de RAM.</li>
</ul>
<h3>MBC2</h3>
<p>Un seul registre en 0000–3FFF. C'est le <b>bit 8 de l'adresse</b> qui décide : à 0, on ouvre ou ferme la RAM ; à 1, on choisit la banque (4 bits). Sa RAM est dans la puce : 512 cases de 4 bits. Les 4 bits hauts se lisent à 1 (<code>0xF0 | valeur</code>), et les 512 cases se répètent sur toute la plage A000–BFFF.</p>
<h3>MBC5</h3>
<p>Numéro de banque sur 9 bits, écrit en deux fois : les 8 bits bas en 2000–2FFF, le bit 8 en 3000–3FFF. Jusqu'à 512 banques, soit 8 Mio. La banque 0 est permise, sans règle « 0 devient 1 ». La banque de RAM tient sur 4 bits.</p>
<h3>Un seul contrat pour tous</h3>
${rs('src/mbc.rs', `
pub trait Mbc {
    fn read_rom(&self, addr: u16) -> u8;
    fn write_rom(&mut self, addr: u16, value: u8); // des commandes, jamais une écriture
    fn read_ram(&self, addr: u16) -> u8;
    fn write_ram(&mut self, addr: u16, value: u8);
}`)}
<p>La cartouche garde un <code>Box&lt;dyn Mbc&gt;</code>. Le bus appelle ces quatre fonctions sans savoir quelle puce répond, comme le CPU de la console.</p>`,
    init: initMbc,
  });

  add({
    id: 'sram', stage: 5, name: 'RAM de la cartouche', hw: 'SRAM, de 2 Kio à 128 Kio', src: 'src/mbc.rs', addr: 'A000–BFFF',
    short: 'La mémoire où le jeu range sa partie.',
    html: `
<p class="lede">Une mémoire en plus, dans la cartouche, visible en A000–BFFF. Les jeux y rangent la partie sauvegardée.</p>
<h3>Un verrou</h3>
<p>Par défaut, cette RAM est fermée : une lecture renvoie <code>0xFF</code>, une écriture est perdue. Le jeu l'ouvre en écrivant une valeur dont le quartet bas vaut <code>0xA</code> dans la plage 0000–1FFF, et la referme après usage. Le but : éviter d'abîmer la sauvegarde quand la console s'éteint.</p>
${st(0x0a, ['–', '–', '–', '–', '1', '0', '1', '0'], [0, 0, 0, 0, 1, 1, 1, 1])}
${rs('src/mbc.rs', `
0x0000..=0x1FFF => self.ram_enabled = value & 0x0F == 0x0A,`)}
<h3>Des banques, là aussi</h3>
<p>La plage ne montre que 8 Kio. Une RAM de 32 Kio est découpée en 4 banques. Une seule formule gère aussi les petites RAM et les banques qui n'existent pas :</p>
${rs('src/mbc.rs', `
fn banked_ram_index(ram: &[u8], bank: usize, addr: u16) -> usize {
    (bank * RAM_BANK_SIZE + (addr - 0xA000) as usize) % ram.len()
}`)}
<p>Sans pile, cette RAM s'efface à l'extinction. <button type="button" class="link" data-open="battery">Voir la pile</button>.</p>`,
  });

  add({
    id: 'battery', stage: 5, name: 'Pile de sauvegarde', hw: 'Pile bouton 3 V', src: 'src/cartridge.rs', addr: '',
    short: 'Sur la console, une pile. Dans RetroEmu, un fichier .sav.',
    html: `
<p class="lede">La pile maintient la RAM de la cartouche sous tension quand la console est éteinte. C'est tout le mécanisme de sauvegarde d'une Game Boy.</p>
<h3>Dans RetroEmu : un fichier</h3>
<p>Le fichier <code>.sav</code>, rangé à côté de la ROM, joue le rôle de la pile. Son contenu est exactement celui de la RAM, octet pour octet.</p>
<ol>
  <li><b>Au chargement</b>, le fichier est recopié dans la RAM. Pas de fichier : c'est une première partie.</li>
  <li><b>Pendant le jeu</b>, rien de spécial : le jeu écrit dans sa RAM.</li>
  <li><b>À la fermeture</b>, ou quand on change de ROM, la RAM est écrite dans le fichier.</li>
</ol>
${rs('src/cartridge.rs', `
pub fn save(&self) -> Result<()> {
    if let Some(path) = &self.save_path {
        std::fs::write(path, self.mbc.ram())?;
    }
    Ok(())
}`)}
<h3>Deux précautions</h3>
<ul>
  <li><b>Un .sav illisible est une erreur.</b> Continuer reviendrait à écraser la sauvegarde avec une RAM vide à la fermeture.</li>
  <li><b>Pas de pile, pas de fichier.</b> Le header dit si la cartouche en a une (types 03, 06, 1B, 1E...). Sans elle, <code>save_path</code> reste à <code>None</code>.</li>
</ul>`,
  });

  GB.order = Object.keys(C);
})();
