/* Compatibility bridge for the GPL tuanpc-desk 1.0.2 browser themes.
   The host still owns monitor routing and the single Panorama surface. */
(() => {
  'use strict';
  // Shared lightweight precipitation layer. It is a separate canvas so WebGL
  // themes do not need a second full-screen shader pass and 2D themes keep
  // their cached background intact.
  const weatherLayer = document.createElement('canvas');
  weatherLayer.id = 'deskland-weather-layer';
  weatherLayer.setAttribute('aria-hidden', 'true');
  weatherLayer.style.cssText = 'position:fixed;inset:0;width:100vw;height:100vh;pointer-events:none;z-index:2;';
  document.body.appendChild(weatherLayer);
  const weatherCtx = weatherLayer.getContext('2d');
  let weatherW = 0, weatherH = 0, weatherDpr = 1, weatherLast = 0;
  const resizeWeather = () => {
    weatherDpr = Math.min(window.devicePixelRatio || 1, 2);
    weatherW = window.innerWidth; weatherH = window.innerHeight;
    weatherLayer.width = Math.max(1, Math.round(weatherW * weatherDpr));
    weatherLayer.height = Math.max(1, Math.round(weatherH * weatherDpr));
    weatherCtx.setTransform(weatherDpr, 0, 0, weatherDpr, 0, 0);
  };
  const noise = n => { const x = Math.sin(n * 12.9898 + 78.233) * 43758.5453; return x - Math.floor(x); };
  const wrap = (v, size) => ((v % size) + size) % size;
  const paintWeather = now => {
    requestAnimationFrame(paintWeather);
    if (now - weatherLast < 50) return; // cap the overlay around 20fps
    weatherLast = now;
    if (!weatherW || !weatherH) resizeWeather();
    const e = DeskWorld.environment, g = weatherCtx;
    g.clearRect(0, 0, weatherW, weatherH);
    if (!e.weatherEnabled) return;
    const time = now * 0.001;
    const windRad = (Number(e.windDirection || 110) * Math.PI) / 180;
    // Keep the storm tint subtle: it should make the scene feel overcast,
    // while the theme artwork and desktop icons remain readable underneath.
    if (e.storm > 0.5) {
      g.fillStyle = `rgba(26,42,64,${0.035 + e.cloudiness * 0.045})`;
      g.fillRect(0, 0, weatherW, weatherH);
    }
    if (e.rain > 0.01) {
      const count = Math.min(170, Math.round(46 + 96 * e.rain + 28 * e.storm));
      const drift = Math.sin(windRad) * (20 + 125 * e.wind);
      const fall = 170 + 150 * e.rain + 115 * e.storm;
      const rainSpan = Math.max(weatherW, weatherH) + 120;
      g.save(); g.lineCap = 'round';
      for (let i = 0; i < count; i++) {
        // A stable depth value gives near drops more length, brightness and
        // width, which removes the flat “same brush” appearance.
        const depth = noise(i * 3.17 + 203.5);
        const x = wrap(noise(i + 7.1) * rainSpan + time * drift + i * 13, rainSpan) - 60;
        const y = wrap(noise(i + 19.7) * rainSpan + time * fall + i * 17, rainSpan) - 60;
        const len = (6 + 10 * e.rain + 14 * e.storm) * (0.55 + depth * 0.95);
        const dx = (drift / fall) * len * (0.55 + depth * 0.45);
        const alpha = (0.08 + depth * 0.24) * (0.72 + e.storm * 0.35);
        g.lineWidth = 0.45 + depth * (e.storm > 0.5 ? 1.15 : 0.8);
        g.strokeStyle = e.storm > 0.5
          ? `rgba(190,215,245,${alpha.toFixed(3)})`
          : `rgba(205,230,250,${(alpha * 0.78).toFixed(3)})`;
        g.beginPath(); g.moveTo(x, y); g.lineTo(x + dx, y + len); g.stroke();
      }
      g.restore();
    }
    if (e.snow > 0.01) {
      const count = Math.min(90, Math.round(24 + 58 * e.snow));
      g.save(); g.fillStyle = 'rgba(255,255,255,.70)';
      for (let i = 0; i < count; i++) {
        const x = (noise(i + 101.3) * weatherW + Math.sin(time * 0.35 + i) * 24 + time * (12 + 20 * e.wind)) % (weatherW + 20) - 10;
        const y = (noise(i + 131.9) * weatherH + time * (18 + 28 * e.snow) + i * 9) % (weatherH + 20) - 10;
        const r = 1.2 + noise(i + 171.4) * 2.1;
        g.beginPath(); g.arc(x, y, r, 0, Math.PI * 2); g.fill();
      }
      g.restore();
    }
    if (e.storm > 0.5 && e.lightning > 0.01) {
      const pulse = e.lightning;
      // A small local bolt sells the flash better than whitening the whole
      // screen. The scene-wide lift stays restrained so it reads as light
      // reflected through clouds rather than a white overlay.
      g.save();
      g.fillStyle = `rgba(220,235,255,${(0.018 + pulse * 0.085).toFixed(3)})`;
      g.fillRect(0, 0, weatherW, weatherH);
      if (pulse > 0.18) {
        const bx = weatherW * (0.18 + noise((e.day || 0) * 0.31 + 34.7) * 0.64);
        const top = weatherH * (0.05 + noise((e.day || 0) * 0.17 + 16.2) * 0.12);
        const bottom = weatherH * (0.46 + noise((e.day || 0) * 0.21 + 91.4) * 0.2);
        const segments = 5;
        g.beginPath(); g.moveTo(bx, top);
        for (let j = 1; j <= segments; j++) {
          const y = top + (bottom - top) * j / segments;
          const x = bx + (noise((e.day || 0) * 0.73 + j * 9.1) - 0.5) * weatherW * 0.16;
          g.lineTo(x, y);
        }
        g.lineJoin = 'round'; g.lineCap = 'round';
        g.shadowColor = `rgba(210,230,255,${(0.42 * pulse).toFixed(3)})`;
        g.shadowBlur = 12;
        g.strokeStyle = `rgba(235,245,255,${(0.65 * pulse).toFixed(3)})`;
        g.lineWidth = 1.1 + pulse * 1.3;
        g.stroke();
      }
      g.restore();
    }
  };
  resizeWeather();
  addEventListener('resize', resizeWeather);
  requestAnimationFrame(paintWeather);
  DeskWorld.invalidate = () => {
    if (!paused) return;
    // A paused wallpaper must still complete new terrain and parameter updates.
    if (typeof curMix !== 'undefined') curMix = 1;
    if (typeof morphT0 !== 'undefined') morphT0 = -1e9;
    if (typeof needDraw !== 'undefined') needDraw = false;
    draw(performance.now());
  };
  window.desklandDiagnostics = () => ({
    ...DeskWorld.diagnostics, paused, seed, icons: icons.length,
    canvas: [cv.width, cv.height], mode: DeskWorld.contract?.mode,
    world: DeskWorld.contract?.world, viewport: DeskWorld.contract?.viewport,
    renderer: typeof gl === 'undefined' ? 'canvas2d' : (gl && !gl.isContextLost() ? 'webgl2' : 'failed'),
    page: location.pathname
  });
})();
