/* Shared renderer contract v1. All host rectangles are physical screen pixels.
   Panorama is a single world surface; local = screen - virtual-desktop origin.
   Mirror surfaces share epoch and seed, while keeping their own aspect ratio. */
(function (root) {
  'use strict';
  let contract = null, epochOffset = 0, randomState = 104729, nextFrameAt = 0;
  let draws = 0, iconsSeen = false;
  const errors = [];

  // Lightweight, offline environmental clock shared by every theme.  The
  // calculation is deliberately location-free: it gives a convincing local
  // day/night rhythm without permissions, network requests, or per-frame work.
  const TAU = Math.PI * 2;
  let environmentCache = null, environmentStamp = 0;
  const clamp01 = v => Math.max(0, Math.min(1, v));
  const smooth = (a, b, x) => {
    const t = clamp01((x - a) / Math.max(1e-6, b - a));
    return t * t * (3 - 2 * t);
  };
  const hashDay = day => {
    let h = Math.imul(day | 0, 0x45d9f3b);
    h = Math.imul(h ^ (h >>> 16), 0x45d9f3b);
    return ((h ^ (h >>> 16)) >>> 0) / 4294967296;
  };
  function environmentAt(value) {
    const date = value instanceof Date ? value : new Date(value == null ? Date.now() : value);
    const h = date.getHours() + date.getMinutes() / 60 + date.getSeconds() / 3600 + date.getMilliseconds() / 3600000;
    const dayProgress = h / 24;
    const day = Math.floor(new Date(date.getFullYear(), date.getMonth(), date.getDate()).getTime() / 86400000);
    const daylight = smooth(5.0, 7.0, h) * (1 - smooth(17.0, 19.5, h));
    const dawn = smooth(4.8, 7.0, h) * (1 - smooth(6.7, 8.1, h));
    const dusk = smooth(16.7, 19.5, h) * (1 - smooth(19.0, 20.8, h));
    const golden = Math.max(dawn, dusk);
    const solarT = clamp01((h - 6.0) / 12.0);
    const sunElevation = daylight > 0 ? Math.sin(solarT * Math.PI) : -0.16 - 0.12 * Math.cos(dayProgress * TAU);
    const sunAzimuth = (90 + solarT * 180 + 360) % 360;
    const noon = Math.max(0, Math.sin((h - 6) / 12 * Math.PI));
    const dayNoise = hashDay(day);
    const windWave = Math.sin(dayProgress * TAU * 2 + dayNoise * TAU) * 0.5 +
      Math.sin(dayProgress * TAU * 5.3 + dayNoise * 9.0) * 0.2;
    const wind = clamp01(0.22 + dayNoise * 0.28 + (windWave + 0.7) * 0.22);
    const gust = clamp01(0.18 + 0.16 * Math.sin(dayProgress * TAU * 3.1 + dayNoise * 15) + 0.08 * Math.sin(dayProgress * TAU * 8.7));
    const windDirection = (dayNoise * 240 + dayProgress * 90 + 360) % 360;
    const cloudiness = clamp01(0.22 + 0.46 * hashDay(day + 17) + 0.12 * Math.sin(dayProgress * TAU + dayNoise * 5));
    const night = 1 - daylight;
    const warm = clamp01(golden * 0.9 + night * 0.08);
    const lightStrength = 0.24 + daylight * (0.55 + noon * 0.21) + golden * 0.08;
    const light = {
      strength: lightStrength,
      r: 0.66 + daylight * 0.28 + golden * 0.09,
      g: 0.73 + daylight * 0.23 + golden * 0.02,
      b: 0.98 - golden * 0.22 - night * 0.10,
    };
    let phase = 'night';
    if (dawn > 0.18) phase = 'dawn';
    else if (dusk > 0.18) phase = 'dusk';
    else if (daylight > 0.52) phase = golden > 0.18 ? 'golden-hour' : 'day';
    return Object.freeze({
      now: date.getTime(), day, dayProgress, hour: h,
      phase, daylight, dawn, dusk, golden, night,
      sunElevation, sunAzimuth, noon,
      light: Object.freeze(light),
      temperatureTint: warm,
      wind, gust, windDirection, cloudiness,
      // convenient normalized values for shader uniforms and theme code
      skyLight: lightStrength,
      isDay: daylight > 0.35,
    });
  }
  function currentEnvironment() {
    const now = Date.now();
    // Refresh at most once per second. Rendering remains free to ask every
    // frame while all date and noise work stays off the hot path.
    if (!environmentCache || now < environmentStamp || now - environmentStamp >= 1000) {
      environmentCache = environmentAt(now);
      environmentStamp = now;
      if (root.dispatchEvent && typeof root.CustomEvent === 'function') {
        try { root.dispatchEvent(new root.CustomEvent('deskland-environment', { detail: environmentCache })); } catch (_) { /* old WebView */ }
      }
    }
    return environmentCache;
  }
  const api = {
    configure(next) {
      if (!next || next.version !== 1) return;
      contract = next;
      epochOffset = Date.now() - performance.now() - next.epoch;
    },
    seconds(now, fallback) { return contract ? (now + epochOffset) / 1000 : (now - fallback) / 1000; },
    frameDue(now) {
      if (now < nextFrameAt) return false;
      nextFrameAt = !nextFrameAt || now - nextFrameAt > 100 ? now + 1000 / 30 : nextFrameAt + 1000 / 30;
      return true;
    },
    resetFrame() { nextFrameAt = 0; },
    noteDraw() { draws++; },
    received(message) {
      if (message.t === 'icons') iconsSeen = true;
      // Run after the page's handler has applied the new geometry/parameters.
      if (['icons', 'settings', 'reset', 'pause'].includes(message.t))
        queueMicrotask(() => { if (api.invalidate) api.invalidate(); });
    },
    get diagnostics() { return { draws, iconsSeen, errors: errors.slice() }; },
    environmentAt,
    get environment() { return currentEnvironment(); },
    refreshEnvironment() { environmentCache = null; return currentEnvironment(); },
    seed(value) { randomState = value >>> 0; },
    makeRandom(value) {
      let state = value >>> 0;
      return () => {
        state = (state + 0x6D2B79F5) >>> 0;
        let t = state;
        t = Math.imul(t ^ t >>> 15, t | 1);
        t ^= t + Math.imul(t ^ t >>> 7, t | 61);
        return ((t ^ t >>> 14) >>> 0) / 4294967296;
      };
    },
    random() {
      randomState = (randomState + 0x6D2B79F5) >>> 0;
      let t = randomState;
      t = Math.imul(t ^ t >>> 15, t | 1);
      t ^= t + Math.imul(t ^ t >>> 7, t | 61);
      return ((t ^ t >>> 14) >>> 0) / 4294967296;
    },
    localRect(r, viewport) { return [r[0] - viewport.x, r[1] - viewport.y, r[2] - viewport.x, r[3] - viewport.y]; },
    cssRect(r, ratio) { return { x:r[0]/ratio, y:r[1]/ratio, w:(r[2]-r[0])/ratio, h:(r[3]-r[1])/ratio }; },
    get contract() { return contract; }
  };
  root.DeskWorld = api;
  if (root.addEventListener) {
    root.addEventListener('error', e => errors.push(String(e.message || 'Resource failed: ' + e.target?.src)));
    root.addEventListener('unhandledrejection', e => errors.push(String(e.reason)));
  }
  if (typeof module !== 'undefined') module.exports = api;
})(globalThis);
