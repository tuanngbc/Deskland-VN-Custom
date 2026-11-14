// One-time migration of pristine imported HTML pages. Shared helper scripts,
// translations and sharing are maintained separately in this repository.
'use strict';
const fs = require('node:fs');
const path = require('node:path');
const root = path.join(__dirname, '..', 'prototype');
const registry = JSON.parse(fs.readFileSync(path.join(root, 'themes.json'), 'utf8'));
for (const theme of registry.themes) {
  const file = path.join(root, theme.page);
  let s = fs.readFileSync(file, 'utf8').replace(/\r\n/g, '\n');
  if (s.includes('theme-runtime.js')) continue;
  s = s.replace(/<script(?=[\s>])/, '<script src="world-contract.js"></script>\n<script');
  s = s.replace(/Math\.random/g, 'DeskWorld.random');
  s = s.replace(/\(DeskWorld\.random\(\) \* 1e6\) \| 0/g, '104729');
  s = s.replace(/let seed = (108|2026),?/g, m => m.replace(/108|2026/, '104729'));
  s = s.replace(/function reseed\((v|value)\) \{\n/g, '$&  DeskWorld.seed($1);\n');
  s = s.replace(/\{ seed = v.seed;/g, '{ DeskWorld.seed(v.seed); seed = v.seed;');
  s = s.replace(/const handle = m => \{/, '$&\n    DeskWorld.received(m);');
  s = s.replace(/if \(m.t === 'settings'\) \{ applySettings/, "if (m.t === 'settings') { DeskWorld.configure(m.contract); applySettings");
  s = s.replace(/paused = !!m.v/g, 'paused = PREVIEW ? false : !!m.v');
  s = s.replace(/lastT = 0; return;/g, 'lastT = 0; DeskWorld.resetFrame(); return;');
  s = s.replace(/Math.min\((?:window\.)?devicePixelRatio \|\| 1, (?:0\.72|0\.85)\)/g, 'Math.min(devicePixelRatio || 1, 2)');
  s = s.replace(/if \((?:P.cap30 && )?now - lastT < (?:40|34|FRAME_MS|\(busy \? 22 : 38\)|\(busy \? 20 : 42\))\) return;[^\n]*/g,
    'if (!DeskWorld.frameDue(now)) return;');
  if (theme.id === 'stadium') {
    s = s.replace('  draw();', '  draw(now);');
    s = s.replace('  const dt = lastT ?', '  if (!DeskWorld.frameDue(now)) return;\n  const dt = lastT ?');
    s = s.replace('function draw() {', 'function draw() {\n  ctx.setTransform(cv._scale, 0, 0, cv._scale, 0, 0);');
    s = s.replace('cv.height = Math.round(H * dpr);', 'cv.height = Math.round(H * dpr); cv._scale = dpr;');
    s = s.replace("    parent.postMessage(JSON.stringify({ t: 'ready' }), '*');\n  }",
      "    parent.postMessage(JSON.stringify({ t: 'ready' }), '*');\n  } else {\n    const host = window.chrome && window.chrome.webview;\n    if (host) { host.addEventListener('message', ev => handle(JSON.parse(ev.data))); host.postMessage('ready'); }\n  }");
  }
  s = s.replace(/function draw\((?:now)?\) \{/, '$&\n  DeskWorld.noteDraw();');
  s = s.replace(/\(now - T0\) \/ 1000/g, 'DeskWorld.seconds(now, T0)');
  if (theme.id === 'cats') s = s.replace('const RENDER_SCALE_MAX = 1;', 'const RENDER_SCALE_MAX = 2;');
  if (theme.id === 'cat-hill') s = s.replace('Math.min(devicePixelRatio || 1, 0.85)', 'Math.min(devicePixelRatio || 1, 2)');
  s = s.replace('W = innerWidth; H = innerHeight;', "const displayBox = cv.getBoundingClientRect();\n  W = displayBox.width || innerWidth; H = displayBox.height || innerHeight;");
  if (!s.includes('src="share.js"')) s = s.replace('</body>', '<script src="share.js"></script>\n</body>');
  s = s.replace('<script src="share.js">', '<script src="theme-runtime.js"></script>\n<script src="share.js">');
  if (!s.includes('DeskWorld.configure(m.contract)') || !s.includes('DeskWorld.frameDue(now)')) throw new Error('Missed port: ' + theme.id);
  fs.writeFileSync(file, s);
}
