/* Compatibility bridge for the GPL Deskland-VN 1.0.2 browser themes.
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
  const paintWeather = now => {
    requestAnimationFrame(paintWeather);
    if (now - weatherLast < 50) return; // cap the overlay around 20fps
    weatherLast = now;
    if (!weatherW || !weatherH) resizeWeather();
    const e = DeskWorld.environment, g = weatherCtx;
    g.clearRect(0, 0, weatherW, weatherH);
    if (!e.weatherEnabled) return;
    const time = now * 0.001;
    if (e.rain > 0.01) {
      const count = Math.min(150, Math.round(42 + 110 * e.rain + 35 * e.storm));
      g.save(); g.lineCap = 'round'; g.lineWidth = e.storm > 0.5 ? 1.4 : 1;
      g.strokeStyle = e.storm > 0.5 ? 'rgba(190,215,245,.45)' : 'rgba(205,230,250,.30)';
      for (let i = 0; i < count; i++) {
        const x = (noise(i + 7.1) * weatherW + time * (55 + 75 * e.wind) + i * 13) % (weatherW + 40) - 20;
        const y = (noise(i + 19.7) * weatherH + time * (240 + 180 * e.wind) + i * 17) % (weatherH + 80) - 40;
        const len = 7 + 8 * e.rain + 7 * e.storm;
        g.beginPath(); g.moveTo(x, y); g.lineTo(x - 3 - e.wind * 4, y + len); g.stroke();
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
    if (e.storm > 0.5 && e.lightning > 0.56) {
      g.fillStyle = `rgba(220,235,255,${Math.min(.18, (e.lightning - .56) * .8)})`;
      g.fillRect(0, 0, weatherW, weatherH);
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
