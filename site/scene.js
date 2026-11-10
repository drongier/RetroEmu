// Scène 3D : la console modélisée en formes simples, éclatée selon l'avancement du scroll.
// Unité : à peu près le centimètre. La face avant regarde vers +z.
(function () {
  const GB = window.GB;
  const api = (GB.scene = { ok: false });
  const canvas = document.getElementById('scene');
  const stageEl = canvas.parentElement;
  if (!window.THREE) return;

  let renderer;
  try {
    renderer = new THREE.WebGLRenderer({ canvas, antialias: true, alpha: true });
  } catch (e) {
    return;
  }
  api.ok = true;
  renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, 2));
  renderer.outputEncoding = THREE.sRGBEncoding;

  const scene = new THREE.Scene();
  const camera = new THREE.PerspectiveCamera(28, 1, 1, 800);
  scene.add(new THREE.HemisphereLight(0xffffff, 0x7c8096, 0.62));
  const key = new THREE.DirectionalLight(0xffffff, 0.55);
  key.position.set(10, 16, 24);
  scene.add(key);
  const rim = new THREE.DirectionalLight(0xffffff, 0.3);
  rim.position.set(-14, 6, -12);
  scene.add(rim);

  const tilt = new THREE.Group(); // inclinaison (pitch)
  const model = new THREE.Group(); // rotation autour de la verticale (yaw)
  tilt.add(model);
  scene.add(tilt);

  // ───────────────────────────── Outils ─────────────────────────────

  const byId = {};
  const anchors = {};
  function tag(obj, id) {
    obj.userData.id = id;
    (byId[id] = byId[id] || []).push(obj);
    if (!anchors[id]) anchors[id] = obj;
    return obj;
  }

  const mat = (color, o) => new THREE.MeshStandardMaterial(Object.assign({ color, roughness: 0.62, metalness: 0.04 }, o));
  const metal = color => mat(color, { roughness: 0.32, metalness: 0.75 });
  const box = (w, h, d, m) => new THREE.Mesh(new THREE.BoxGeometry(w, h, d), m);
  function put(obj, parent, x, y, z) {
    obj.position.set(x || 0, y || 0, z || 0);
    parent.add(obj);
    return obj;
  }

  /** Trace un rectangle arrondi centré sur (cx, cy). `rbr` : rayon du coin bas-droit. */
  function rr(path, w, h, r, rbr, cx, cy) {
    const x = (cx || 0) - w / 2, y = (cy || 0) - h / 2;
    if (rbr === undefined) rbr = r;
    path.moveTo(x + r, y);
    path.lineTo(x + w - rbr, y);
    path.quadraticCurveTo(x + w, y, x + w, y + rbr);
    path.lineTo(x + w, y + h - r);
    path.quadraticCurveTo(x + w, y + h, x + w - r, y + h);
    path.lineTo(x + r, y + h);
    path.quadraticCurveTo(x, y + h, x, y + h - r);
    path.lineTo(x, y + r);
    path.quadraticCurveTo(x, y, x + r, y);
    return path;
  }
  const circle = (cx, cy, r) => {
    const p = new THREE.Path();
    p.absarc(cx, cy, r, 0, Math.PI * 2, true);
    return p;
  };
  const extrude = (shape, depth, m) =>
    new THREE.Mesh(new THREE.ExtrudeGeometry(shape, { depth, bevelEnabled: false, curveSegments: 14 }), m);
  const cyl = (r, h, m, seg) => {
    const c = new THREE.Mesh(new THREE.CylinderGeometry(r, r, h, seg || 32), m);
    c.rotation.x = Math.PI / 2; // l'axe du cylindre regarde vers +z
    return c;
  };

  function texture(w, h, draw) {
    const c = document.createElement('canvas');
    c.width = w;
    c.height = h;
    draw(c.getContext('2d'), w, h);
    const t = new THREE.CanvasTexture(c);
    t.encoding = THREE.sRGBEncoding;
    t.anisotropy = 4;
    return t;
  }

  /** Plan portant quelques lignes de texte, ajustées à sa largeur. */
  function label(w, h, lines, bg, fg, o) {
    o = o || {};
    const k = 110;
    const t = texture(Math.max(64, Math.round(w * k)), Math.max(64, Math.round(h * k)), (g, W, H) => {
      if (bg) {
        g.fillStyle = bg;
        g.fillRect(0, 0, W, H);
      }
      g.fillStyle = fg;
      g.textAlign = 'center';
      g.textBaseline = 'middle';
      const lh = H / (lines.length + 0.6);
      lines.forEach((line, i) => {
        let size = Math.min(lh * (i === 0 ? 0.78 : 0.56), H * (lines.length === 1 ? 0.7 : 0.42));
        const font = s => `${o.italic ? 'italic ' : ''}${i === 0 ? 700 : 500} ${s}px "IBM Plex Mono", Menlo, Consolas, monospace`;
        g.font = font(size);
        const m = g.measureText(line).width;
        if (m > W * 0.9) size *= (W * 0.9) / m;
        g.font = font(size);
        g.fillText(line, W / 2, lh * (i + 0.8));
      });
    });
    const m = new THREE.MeshStandardMaterial({ map: t, roughness: 0.55, transparent: !bg });
    return new THREE.Mesh(new THREE.PlaneGeometry(w, h), m);
  }

  /** Puce : boîtier noir, marquage sur le dessus, pattes sur 2 ou 4 côtés. */
  function chip(w, h, lines, id, o) {
    o = o || {};
    const g = new THREE.Group();
    const t = 0.26;
    g.add(box(w, h, t, mat(0x1b1c22, { roughness: 0.45 })));
    put(label(w * 0.94, h * 0.94, lines, '#1b1c22', '#d8d5c8'), g, 0, 0, t / 2 + 0.004);
    const legs = metal(0xbfc2ca);
    put(box(w + 0.36, h * 0.84, 0.05, legs), g, 0, 0, -t / 2 + 0.03);
    if (o.quad) put(box(w * 0.84, h + 0.36, 0.05, legs), g, 0, 0, -t / 2 + 0.03);
    return id ? tag(g, id) : g;
  }

  // ───────────────────────────── La console ─────────────────────────────

  const W = 9, H = 14.8;
  const PLASTIC = 0xbdb9ae, PCB = 0x1f6b4a;
  const L = {};
  ['front', 'buttons', 'lcd', 'main', 'cart', 'back'].forEach(k => {
    L[k] = new THREE.Group();
    model.add(L[k]);
  });

  // Coque avant : une paroi, et une façade percée pour l'écran et les boutons.
  (function () {
    const wall = rr(new THREE.Shape(), W, H, 0.6, 2.0);
    wall.holes.push(rr(new THREE.Path(), W - 0.5, H - 0.5, 0.45, 1.8));
    put(extrude(wall, 1.2, mat(PLASTIC)), L.front, 0, 0, 0.1);
    const face = rr(new THREE.Shape(), W, H, 0.6, 2.0);
    face.holes.push(rr(new THREE.Path(), 5.3, 4.9, 0.05, 0.05, 0, 4.0), circle(-2.5, -1.6, 1.22), circle(3.1, -1.2, 0.7), circle(1.5, -2.0, 0.7));
    put(extrude(face, 0.3, mat(PLASTIC)), L.front, 0, 0, 1.3);
    // Contour sombre de l'écran.
    const lens = rr(new THREE.Shape(), 7.5, 5.7, 0.3, 1.3, 0, 4.0);
    lens.holes.push(rr(new THREE.Path(), 4.8, 4.4, 0.05, 0.05, 0, 4.0));
    put(extrude(lens, 0.06, mat(0x5d5e6b, { roughness: 0.35 })), L.front, 0, 0, 1.6);
    put(box(5.2, 0.05, 0.02, mat(0xa3195b)), L.front, 0, 6.52, 1.67);
    put(box(5.2, 0.05, 0.02, mat(0x2a2d7a)), L.front, 0, 6.4, 1.67);
    put(label(3.0, 0.7, ['RetroEmu'], null, '#2a2d7a', { italic: true }), L.front, -2.2, 0.5, 1.61);
    // Grille du haut-parleur.
    const grille = new THREE.Group();
    for (let i = 0; i < 6; i++) put(box(0.13, 1.7, 0.04, mat(0x8d8a82)), grille, (i - 2.5) * 0.36, 0, 0);
    grille.rotation.z = -0.5;
    put(grille, L.front, 2.75, -5.6, 1.6);
  })();

  // Boutons.
  (function () {
    const dpad = tag(new THREE.Group(), 'joypad');
    const dark = () => mat(0x26272e, { roughness: 0.5 });
    dpad.add(box(2.05, 0.66, 0.5, dark()), box(0.66, 2.05, 0.5, dark()));
    put(dpad, L.buttons, -2.5, -1.6, 1.45);
    [[3.1, -1.2], [1.5, -2.0]].forEach(([x, y]) => {
      put(tag(cyl(0.6, 0.5, mat(0x9a1d58, { roughness: 0.45 })), 'joypad'), L.buttons, x, y, 1.45);
    });
    [-1.0, 0.55].forEach(x => {
      const pill = tag(extrude(rr(new THREE.Shape(), 1.15, 0.36, 0.18), 0.16, mat(0x77757b)), 'joypad');
      pill.rotation.z = 0.45;
      put(pill, L.buttons, x, -4.3, 1.6);
    });
  })();

  // Carte de l'écran : dalle LCD, contacts des boutons, haut-parleur.
  (function () {
    put(extrude(rr(new THREE.Shape(), 8.2, 13.8, 0.3), 0.12, mat(PCB)), L.lcd, 0, 0, 0.5);
    const lcd = tag(new THREE.Group(), 'lcd');
    lcd.add(box(6.0, 5.4, 0.34, metal(0xaeb1b8)));
    const SH = ['#c6d18c', '#8fa558', '#4a6b3a', '#1f3a26'];
    const FONT = {
      R: '11110100011000111110101001001010001', E: '11111100001000011110100001000011111', T: '11111001000010000100001000010000100',
      O: '01110100011000110001100011000101110', M: '10001110111010110101100011000110001', U: '10001100011000110001100011000101110',
    };
    const screenTex = texture(160, 144, g => {
      g.fillStyle = SH[0];
      g.fillRect(0, 0, 160, 144);
      g.fillStyle = SH[1];
      [[12, 14, 30], [108, 22, 38], [58, 8, 22]].forEach(([x, y, w]) => {
        g.fillRect(x, y, w, 6);
        g.fillRect(x + 4, y - 4, w - 10, 4);
      });
      g.fillStyle = SH[3];
      [...'RETROEMU'].forEach((ch, i) => {
        for (let k = 0; k < 35; k++) if (FONT[ch][k] === '1') g.fillRect(33 + i * 12 + (k % 5) * 2, 44 + Math.floor(k / 5) * 2, 2, 2);
      });
      g.fillStyle = SH[2];
      g.fillRect(33, 62, 94, 2);
      // Sol en tuiles 8 × 8.
      for (let ty = 0; ty < 4; ty++) {
        for (let tx = 0; tx < 20; tx++) {
          const x = tx * 8, y = 112 + ty * 8;
          g.fillStyle = ty === 0 ? SH[2] : SH[1];
          g.fillRect(x, y, 8, 8);
          g.fillStyle = ty === 0 ? SH[3] : SH[2];
          g.fillRect(x, y, 8, 1);
          g.fillRect(x + ((tx + ty) % 2 ? 0 : 4), y, 1, 8);
        }
      }
      // Un sprite : une petite cartouche.
      g.fillStyle = SH[3];
      g.fillRect(72, 92, 16, 20);
      g.fillStyle = SH[1];
      g.fillRect(75, 95, 10, 8);
      g.fillStyle = SH[0];
      g.fillRect(77, 106, 2, 4);
      g.fillRect(81, 106, 2, 4);
    });
    screenTex.magFilter = THREE.NearestFilter;
    const screen = new THREE.Mesh(new THREE.PlaneGeometry(4.8, 4.32), new THREE.MeshStandardMaterial({ map: screenTex, roughness: 0.4 }));
    put(screen, lcd, 0, 0, 0.18);
    put(lcd, L.lcd, 0, 4.0, 0.8);
    // Contacts dorés sous les boutons.
    const gold = () => metal(0xc9a44a);
    [[-2.5, -1.6, 1.0], [3.1, -1.2, 0.55], [1.5, -2.0, 0.55], [-0.75, -4.2, 0.3], [0.8, -4.2, 0.3]].forEach(([x, y, r]) => {
      put(cyl(r, 0.03, gold()), L.lcd, x, y, 0.64);
    });
    const spk = tag(new THREE.Group(), 'apu');
    spk.add(cyl(1.15, 0.3, mat(0x2b2c33)));
    put(cyl(0.75, 0.06, mat(0x55565f)), spk, 0, 0, 0.17);
    put(cyl(0.3, 0.1, metal(0x9a9da6)), spk, 0, 0, 0.2);
    put(spk, L.lcd, 2.6, -5.3, 0.78);
  })();

  // Carte mère.
  const SOC = { x: 0, y: -1.5, z: -0.2 };
  let die, callout;
  (function () {
    const board = tag(new THREE.Group(), 'bus');
    put(extrude(rr(new THREE.Shape(), 8.2, 10.4, 0.3), 0.12, mat(PCB)), board, 0, -1.8, -0.45);
    // Pistes : le bus qui relie les puces.
    const trace = () => mat(0x4fae7c, { roughness: 0.4 });
    for (let i = 0; i < 7; i++) {
      put(box(1.5, 0.045, 0.012, trace()), board, -1.95, -0.6 - i * 0.3, -0.324);
      put(box(1.5, 0.045, 0.012, trace()), board, 1.95, -0.6 - i * 0.3, -0.324);
      put(box(0.045, 2.4, 0.012, trace()), board, -0.9 + i * 0.3, 1.2, -0.324);
    }
    for (let i = 0; i < 5; i++) put(box(0.045, 1.6, 0.012, trace()), board, -1.2 + i * 0.6, -4.2, -0.324);
    put(board, L.main);
    put(new THREE.Object3D(), board, 1.9, 1.4, -0.3);
    anchors.bus = board.children[board.children.length - 1];

    put(chip(2.6, 2.6, ['DMG-CPU', 'LR35902', 'SHARP'], 'soc', { quad: true }), L.main, SOC.x, SOC.y, SOC.z);
    put(chip(1.3, 2.7, ['VRAM', 'LH5264'], 'vram'), L.main, -3.2, SOC.y, SOC.z);
    put(chip(1.3, 2.7, ['WRAM', 'LH5264'], 'wram'), L.main, 3.2, SOC.y, SOC.z);
    const quartz = tag(new THREE.Group(), 'quartz');
    quartz.add(extrude(rr(new THREE.Shape(), 1.3, 0.5, 0.25), 0.36, metal(0xc4c7cf)));
    put(quartz, L.main, -2.6, 1.5, -0.33);
    const port = tag(new THREE.Group(), 'serial');
    port.add(box(1.0, 0.9, 0.6, metal(0x9da0a9)));
    put(box(0.7, 0.5, 0.3, mat(0x1b1c22)), port, 0.2, 0, 0.16);
    put(port, L.main, 3.55, 2.3, -0.05);
    // Connecteur de cartouche et quelques composants passifs.
    put(box(6.4, 0.8, 0.6, mat(0x3a3b44)), L.main, 0, 3.0, -0.75);
    [[-3.3, -4.6], [-2.6, -5.2], [2.2, -4.4], [3.2, -5.4], [0.2, -5.8], [-0.9, 2.4], [1.0, 2.6]].forEach(([x, y], i) => {
      put(box(0.34, 0.2, 0.14, mat(i % 2 ? 0x8a6a3a : 0x2b2c33)), L.main, x, y, -0.26);
    });

    // Le silicium de la puce : sort du boîtier et grandit quand on « ouvre » la puce.
    die = new THREE.Group();
    die.add(extrude(rr(new THREE.Shape(), 8, 8, 0.2), 0.12, mat(0x1d2147, { roughness: 0.35, metalness: 0.3 })));
    const BLOCKS = [
      ['cpu', -3.7, 0.3, 3.9, 3.4, ['CPU', 'SM83'], '#3f4fc4'],
      ['ppu', 0.5, -0.6, 3.2, 4.3, ['PPU', 'vidéo'], '#2f8156'],
      ['timer', -3.7, -1.5, 1.8, 1.5, ['TIMER'], '#b97a12'],
      ['interrupts', -1.6, -1.5, 1.8, 1.5, ['IE · IF', 'interruptions'], '#b83a3a'],
      ['oam', 0.5, -2.2, 1.5, 1.3, ['OAM'], '#237d86'],
      ['hram', 2.2, -2.2, 1.5, 1.3, ['HRAM'], '#6a4fb0'],
      ['bootrom', -3.7, -3.7, 1.8, 1.9, ['BOOT', 'ROM'], '#565b7c'],
      ['apu', -1.6, -3.7, 1.8, 1.9, ['APU', 'son'], '#3a3d55'],
      ['serial', 0.5, -3.7, 1.5, 1.2, ['SÉRIE'], '#3a3d55'],
      ['joypad', 2.2, -3.7, 1.5, 1.2, ['P1'], '#3a3d55'],
    ];
    BLOCKS.forEach(([id, x, y, w, h, lines, color]) => {
      const b = new THREE.Group();
      b.add(box(w, h, 0.2, mat(color, { roughness: 0.5 })));
      put(label(w * 0.92, h * 0.92, lines, color, '#f1efe4'), b, 0, 0, 0.104);
      put(b, die, x + w / 2, y + h / 2, 0.22);
      tag(b, id);
    });
    L.main.add(die);
    callout = new THREE.LineSegments(
      new THREE.BufferGeometry().setAttribute('position', new THREE.BufferAttribute(new Float32Array(12), 3)),
      new THREE.LineBasicMaterial({ color: 0xa3195b })
    );
    callout.frustumCulled = false;
    L.main.add(callout);
  })();

  // Cartouche : le circuit reste en place, le boîtier se soulève.
  let cartShell;
  (function () {
    const cy = 3.9, cz = -1.05;
    const pcb = new THREE.Group();
    pcb.add(extrude(rr(new THREE.Shape(), 5.1, 5.4, 0.15), 0.1, mat(0x1e6a55)));
    for (let i = 0; i < 16; i++) put(box(0.16, 0.6, 0.02, metal(0xc9a44a)), pcb, (i - 7.5) * 0.28, -2.35, 0.11);
    put(chip(2.5, 1.5, ['ROM', 'le jeu'], 'rom'), pcb, -1.0, 1.6, 0.24);
    put(chip(1.25, 1.25, ['MBC'], 'mbc', { quad: true }), pcb, 1.55, 1.6, 0.24);
    put(chip(1.3, 1.9, ['SRAM'], 'sram'), pcb, -1.55, -0.65, 0.24);
    const cell = tag(new THREE.Group(), 'battery');
    cell.add(cyl(0.85, 0.22, metal(0xc8cad1), 40));
    put(label(1.0, 0.5, ['3 V +'], null, '#3a3b44'), cell, 0, 0, 0.115);
    put(cell, pcb, 1.1, -0.6, 0.22);
    put(pcb, L.cart, 0, cy - 0.3, cz - 0.1);

    cartShell = new THREE.Group();
    cartShell.add(box(5.8, 6.4, 0.9, mat(0xa9a79f)));
    const art = texture(420, 380, (g, w, h) => {
      g.fillStyle = '#e9e6da';
      g.fillRect(0, 0, w, h);
      g.fillStyle = '#2a2d7a';
      g.fillRect(0, 0, w, 86);
      g.fillStyle = '#e9e6da';
      g.font = 'italic 800 46px Archivo, Arial, sans-serif';
      g.textAlign = 'center';
      g.fillText('RetroEmu', w / 2, 60);
      g.fillStyle = '#a3195b';
      for (let i = 0; i < 8; i++) for (let j = 0; j < 5; j++) if ((i * 3 + j * 5) % 4 < 2) g.fillRect(74 + i * 34, 118 + j * 34, 30, 30);
      g.fillStyle = '#2a2d7a';
      g.font = '600 24px "IBM Plex Mono", Menlo, monospace';
      g.fillText('MBC1 + RAM + PILE', w / 2, 340);
    });
    art.anisotropy = 4;
    put(new THREE.Mesh(new THREE.PlaneGeometry(4.6, 4.2), new THREE.MeshStandardMaterial({ map: art, roughness: 0.5 })), cartShell, 0, -0.5, 0.455);
    put(cartShell, L.cart, 0, cy, cz);
    cartShell.userData.y0 = cy;
  })();

  // Coque arrière et piles.
  (function () {
    const wall = rr(new THREE.Shape(), W, H, 0.6, 2.0);
    wall.holes.push(rr(new THREE.Path(), W - 0.5, H - 0.5, 0.45, 1.8));
    put(extrude(wall, 1.5, mat(PLASTIC)), L.back, 0, 0, -1.6);
    put(extrude(rr(new THREE.Shape(), W, H, 0.6, 2.0), 0.25, mat(PLASTIC)), L.back, 0, 0, -1.6);
    for (let i = 0; i < 4; i++) {
      const aa = new THREE.Mesh(new THREE.CylinderGeometry(0.36, 0.36, 4.6, 24), mat(i % 2 ? 0xc9a23a : 0x2f3340, { roughness: 0.4 }));
      put(aa, L.back, (i - 1.5) * 0.8, -4.3, -0.95);
      put(new THREE.Mesh(new THREE.CylinderGeometry(0.14, 0.14, 0.2, 16), metal(0xc8cad1)), L.back, (i - 1.5) * 0.8, i % 2 ? -1.9 : -6.7, -0.95);
    }
  })();

  // ───────────────────────────── Chorégraphie ─────────────────────────────

  // e : éclatement · side : les étages avant glissent sur le côté pour dégager la vue · die, cart : ouverture de la puce, de la cartouche
  // f : point visé (repère de la console) · d : distance de la caméra
  const K = [
    { p: 0.0, e: 0, side: 0, die: 0, cart: 0, yaw: -0.5, pitch: 0.1, f: [0, 0, 0], d: 46 },
    { p: 0.05, e: 0, side: 0, die: 0, cart: 0, yaw: -0.5, pitch: 0.1, f: [0, 0, 0], d: 46 },
    { p: 0.2, e: 1, side: 0, die: 0, cart: 0, yaw: -0.95, pitch: 0.2, f: [0, 1.5, 1.5], d: 88 },
    { p: 0.32, e: 1, side: 0, die: 0, cart: 0, yaw: -0.95, pitch: 0.2, f: [0, 1.5, 1.5], d: 88 },
    { p: 0.4, e: 1, side: 0, die: 0, cart: 0, yaw: -0.8, pitch: 0.16, f: [0, 0, 14], d: 70 },
    { p: 0.5, e: 1, side: 0, die: 0, cart: 0, yaw: -0.8, pitch: 0.16, f: [0, 0, 14], d: 70 },
    { p: 0.57, e: 1, side: 15, die: 0, cart: 0, yaw: -0.62, pitch: 0.2, f: [0, -1.6, 0], d: 36 },
    { p: 0.66, e: 1, side: 15, die: 0, cart: 0, yaw: -0.62, pitch: 0.2, f: [0, -1.6, 0], d: 36 },
    { p: 0.73, e: 1, side: 15, die: 1, cart: 0, yaw: -0.36, pitch: 0.1, f: [0, 9.4, 0.5], d: 37 },
    { p: 0.82, e: 1, side: 15, die: 1, cart: 0, yaw: -0.36, pitch: 0.1, f: [0, 9.4, 0.5], d: 37 },
    { p: 0.88, e: 1, side: 7, die: 1, cart: 1, yaw: -0.8, pitch: 0.14, f: [0, 10.5, -9], d: 35 },
    { p: 0.94, e: 1, side: 7, die: 1, cart: 1, yaw: -0.8, pitch: 0.14, f: [0, 10.5, -9], d: 35 },
    { p: 0.985, e: 1, side: 0, die: 1, cart: 1, yaw: -0.95, pitch: 0.2, f: [0, 5, 1.5], d: 100 },
    { p: 1.0, e: 1, side: 0, die: 1, cart: 1, yaw: -0.95, pitch: 0.2, f: [0, 5, 1.5], d: 100 },
  ];
  const NUM = ['e', 'side', 'die', 'cart', 'yaw', 'pitch', 'd'];
  const lerp = (a, b, t) => a + (b - a) * t;

  function sample(p) {
    let i = 0;
    while (i < K.length - 2 && p > K[i + 1].p) i++;
    const a = K[i], b = K[i + 1];
    let t = Math.min(1, Math.max(0, (p - a.p) / (b.p - a.p || 1)));
    t = t * t * (3 - 2 * t);
    const s = { f: a.f.map((v, k) => lerp(v, b.f[k], t)) };
    NUM.forEach(k => (s[k] = lerp(a[k], b[k], t)));
    return s;
  }

  let target = sample(0);
  const cur = sample(0);
  const drag = { yaw: 0, pitch: 0 };
  let shift = 0, shiftTarget = 0;
  let stage = 0;
  let size = { w: 1, h: 1 };
  const v3 = new THREE.Vector3();

  function layout(s) {
    L.front.position.set(-s.side, 0, 28 * s.e);
    L.buttons.position.set(-s.side, 0, 13 * s.e);
    L.lcd.position.set(-s.side, 0, 6 * s.e);
    L.cart.position.set(0, 3 * s.e, -9 * s.e);
    L.back.position.z = -17 * s.e;
    cartShell.position.y = cartShell.userData.y0 + 7 * s.cart;

    die.visible = callout.visible = s.die > 0.01;
    if (die.visible) {
      const k = 0.3 + 0.7 * s.die;
      die.scale.setScalar(k);
      die.position.set(SOC.x, SOC.y + 12 * s.die, SOC.z + 0.15 + 0.5 * s.die);
      const a = callout.geometry.attributes.position;
      [-1, 1].forEach((side, i) => {
        a.setXYZ(i * 2, SOC.x + side * 1.3, SOC.y + 1.3, SOC.z + 0.14);
        a.setXYZ(i * 2 + 1, die.position.x + side * 4 * k, die.position.y - 4 * k, die.position.z);
      });
      a.needsUpdate = true;
    }

    model.rotation.y = s.yaw + drag.yaw;
    tilt.rotation.x = s.pitch + drag.pitch;
    tilt.updateMatrixWorld(true);
    v3.set(s.f[0], s.f[1], s.f[2]);
    model.localToWorld(v3);
    const fit = Math.max(1, 1.35 / camera.aspect);
    camera.position.set(v3.x, v3.y, v3.z + s.d * fit);
    camera.lookAt(v3);
  }

  function resize() {
    const w = stageEl.clientWidth, h = stageEl.clientHeight;
    if (!w || !h) return;
    size = { w, h };
    renderer.setSize(w, h, false);
    camera.aspect = w / h;
    applyShift();
  }
  function applyShift() {
    // Décale la console vers la gauche quand la fiche occupe la droite de l'écran.
    // Et toujours un peu vers le haut : le bas de l'écran est réservé à la légende.
    camera.setViewOffset(size.w, size.h, shift, size.h * 0.07, size.w, size.h);
    camera.updateProjectionMatrix();
  }

  // ───────────────────────────── Sélection ─────────────────────────────

  const ray = new THREE.Raycaster();
  const ndc = new THREE.Vector2();
  let hovered = null, selected = null;

  function shown(o) {
    for (; o; o = o.parent) if (!o.visible) return false;
    return true;
  }
  function pickAt(x, y) {
    const r = canvas.getBoundingClientRect();
    ndc.set(((x - r.left) / r.width) * 2 - 1, -((y - r.top) / r.height) * 2 + 1);
    ray.setFromCamera(ndc, camera);
    for (const hit of ray.intersectObject(model, true)) {
      if (hit.object.isLine || !shown(hit.object)) continue;
      for (let o = hit.object; o; o = o.parent) if (o.userData.id) return o.userData.id;
      return null; // une pièce sans fiche (coque...) masque ce qui est derrière
    }
    return null;
  }
  function glow(id, level) {
    if (!id || !byId[id]) return;
    const accent = getComputedStyle(document.documentElement).getPropertyValue('--accent').trim() || '#a3195b';
    byId[id].forEach(root =>
      root.traverse(o => {
        if (o.isMesh && o.material.emissive) {
          o.material.emissive.set(level ? accent : '#000000');
          o.material.emissiveIntensity = level;
        }
      })
    );
  }
  const level = id => (id === selected ? 0.55 : id === hovered ? 0.35 : 0);
  function setHover(id) {
    if (id === hovered) return;
    const old = hovered;
    hovered = id;
    glow(old, level(old));
    glow(id, level(id));
    canvas.style.cursor = id ? 'pointer' : '';
  }
  api.select = function (id) {
    const old = selected;
    selected = id;
    glow(old, level(old));
    glow(id, level(id));
    hs.forEach(h => h.el.classList.toggle('on', h.id === id));
  };

  let down = null, pointer = null;
  canvas.addEventListener('pointerdown', e => {
    down = { x: e.clientX, y: e.clientY, moved: false, touch: e.pointerType === 'touch' };
  });
  window.addEventListener('pointermove', e => {
    if (down) {
      const dx = e.clientX - down.x, dy = e.clientY - down.y;
      if (!down.moved && Math.hypot(dx, dy) > 5) down.moved = true;
      if (down.moved) {
        drag.yaw = Math.max(-1.3, Math.min(1.3, drag.yaw + e.movementX * 0.006));
        if (!down.touch) drag.pitch = Math.max(-0.5, Math.min(0.6, drag.pitch + e.movementY * 0.004));
      }
    }
    pointer = e.target === canvas && e.pointerType !== 'touch' ? { x: e.clientX, y: e.clientY } : null;
  });
  window.addEventListener('pointerup', e => {
    if (down && !down.moved && e.target === canvas) {
      const id = pickAt(e.clientX, e.clientY);
      if (id && api.onPick) api.onPick(id);
    }
    down = null;
  });
  window.addEventListener('pointercancel', () => (down = null));
  canvas.addEventListener('pointerleave', () => (pointer = null));

  // Points cliquables posés sur les pièces.
  const layer = document.getElementById('hotspots');
  const hs = GB.order
    .filter(id => anchors[id])
    .map(id => {
      const c = GB.components[id];
      const el = document.createElement('button');
      el.type = 'button';
      el.className = 'hs';
      el.innerHTML = `<span class="dot"></span><span class="tx">${c.name}</span>`;
      el.addEventListener('click', () => api.onPick && api.onPick(id));
      el.addEventListener('pointerenter', () => setHover(id));
      el.addEventListener('pointerleave', () => setHover(null));
      el.hidden = true;
      layer.appendChild(el);
      return { id, el, stage: c.stage, obj: anchors[id] };
    });

  function hotspots() {
    for (const h of hs) {
      const open = (h.stage !== 4 || cur.die > 0.6) && (h.stage !== 5 || cur.cart > 0.6);
      const here = cur.e > 0.85 && open && shown(h.obj) && (stage === h.stage || stage === 6 || stage === 1);
      if (h.el.hidden === here) h.el.hidden = !here;
      if (!here) continue;
      h.obj.getWorldPosition(v3).project(camera);
      h.el.style.transform = `translate(${((v3.x * 0.5 + 0.5) * size.w).toFixed(1)}px, ${((-v3.y * 0.5 + 0.5) * size.h).toFixed(1)}px)`;
    }
  }

  // ───────────────────────────── Boucle ─────────────────────────────

  let lastP = 0, scrolledAt = -1e9;
  api.setProgress = function (p, stageIndex, instant) {
    if (Math.abs(p - lastP) > 0.0005) scrolledAt = performance.now();
    lastP = p;
    target = sample(p);
    stage = stageIndex;
    if (instant) Object.assign(cur, sample(p));
  };
  api.setPanel = function (px) {
    shiftTarget = px / 2;
  };
  api.resetView = function () {
    drag.yaw = drag.pitch = 0;
  };

  let active = true;
  new IntersectionObserver(es => (active = es[0].isIntersecting)).observe(stageEl);
  window.addEventListener('resize', resize);
  resize();

  function frame() {
    requestAnimationFrame(frame);
    if (!active) return;
    NUM.forEach(k => (cur[k] = lerp(cur[k], target[k], 0.14)));
    cur.f = cur.f.map((v, k) => lerp(v, target.f[k], 0.14));
    if (!down && performance.now() - scrolledAt < 250) {
      // Pendant le scroll, la vue revient dans l'axe prévu pour l'étape.
      drag.yaw *= 0.9;
      drag.pitch *= 0.9;
    }
    if (Math.abs(shift - shiftTarget) > 0.5) {
      shift = lerp(shift, shiftTarget, 0.16);
      applyShift();
    }
    layout(cur);
    if (pointer && !down) setHover(pickAt(pointer.x, pointer.y));
    else if (!pointer && hovered && !layer.matches(':hover')) setHover(null);
    hotspots();
    renderer.render(scene, camera);
  }
  frame();
})();
