/* Shared renderer contract v1. All host rectangles are physical screen pixels.
   Panorama is a single world surface; local = screen - virtual-desktop origin.
   Mirror surfaces share epoch and seed, while keeping their own aspect ratio. */
(function (root) {
  'use strict';
  let contract = null, epochOffset = 0, randomState = 104729, nextFrameAt = 0, targetFps = 30;
  let draws = 0, iconsSeen = false;
  const errors = [];

  // Lightweight environmental clock shared by every theme. The default is a
  // location-free offline simulation; an explicit saved location can opt into
  // a low-frequency live weather request with the same offline fallback.
  const TAU = Math.PI * 2;
  let environmentCache = null, environmentStamp = 0;
  let environmentOptions = { time: true, weather: true, preset: 'auto', source: 'local', location: null };
  let liveWeather = null, liveRequest = null, liveNextTryAt = 0;
  const WEATHER_PRESETS = new Set(['auto', 'clear', 'cloudy', 'rain', 'storm', 'snow']);
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
  const normalizeEnvironmentOptions = options => {
    const o = options || {};
    const preset = WEATHER_PRESETS.has(o.preset) ? o.preset : 'auto';
    const source = o.source === 'live' ? 'live' : 'local';
    const raw = o.location && typeof o.location === 'object' ? o.location : null;
    const latitude = raw ? Number(raw.latitude) : NaN;
    const longitude = raw ? Number(raw.longitude) : NaN;
    const location = Number.isFinite(latitude) && Number.isFinite(longitude) && latitude >= -90 && latitude <= 90 && longitude >= -180 && longitude <= 180
      ? Object.freeze({ latitude, longitude, name: String(raw.name || ''), timezone: String(raw.timezone || '') }) : null;
    return { time: o.time !== false, weather: o.weather !== false, preset, source, location };
  };
  const locationKey = location => location ? `${location.latitude.toFixed(4)},${location.longitude.toFixed(4)}` : '';
  const requestLiveWeather = options => {
    if (options.source !== 'live' || !options.weather || !options.location || typeof root.fetch !== 'function') return;
    const key = locationKey(options.location), now = Date.now();
    if (liveRequest || now < liveNextTryAt || (liveWeather && liveWeather.key === key && now - liveWeather.fetchedAt < 15 * 60 * 1000)) return;
    const q = new URLSearchParams({ latitude: String(options.location.latitude), longitude: String(options.location.longitude), timezone: 'auto',
      current: 'temperature_2m,precipitation,rain,showers,snowfall,weather_code,cloud_cover,wind_speed_10m,wind_gusts_10m,wind_direction_10m,is_day' });
    liveNextTryAt = now + 60 * 1000;
    liveRequest = root.fetch(`https://api.open-meteo.com/v1/forecast?${q.toString()}`).then(response => {
      if (!response.ok) throw new Error(`weather HTTP ${response.status}`);
      return response.json();
    }).then(payload => {
      const c = payload && payload.current;
      if (!c) throw new Error('weather response has no current data');
      liveWeather = { key, fetchedAt: Date.now(), current: c };
      liveNextTryAt = liveWeather.fetchedAt + 15 * 60 * 1000;
      environmentCache = null;
      if (root.dispatchEvent && typeof root.CustomEvent === 'function') {
        try { root.dispatchEvent(new root.CustomEvent('deskland-environment', { detail: currentEnvironment() })); } catch (_) { /* old WebView */ }
      }
    }).catch(() => { /* keep the local simulation as an offline fallback */ })
      .finally(() => { liveRequest = null; });
  };
  const liveWeatherFor = options => {
    const key = locationKey(options.location);
    return liveWeather && liveWeather.key === key ? liveWeather.current : null;
  };
  function environmentAt(value, options) {
    const opts = normalizeEnvironmentOptions(options || environmentOptions);
    requestLiveWeather(opts);
    const date = value instanceof Date ? value : new Date(value == null ? Date.now() : value);
    const clockHour = date.getHours() + date.getMinutes() / 60 + date.getSeconds() / 3600 + date.getMilliseconds() / 3600000;
    const h = opts.time ? clockHour : 12;
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
    let wind = clamp01(0.22 + dayNoise * 0.28 + (windWave + 0.7) * 0.22);
    let gust = clamp01(0.18 + 0.16 * Math.sin(dayProgress * TAU * 3.1 + dayNoise * 15) + 0.08 * Math.sin(dayProgress * TAU * 8.7));
    let windDirection = (dayNoise * 240 + dayProgress * 90 + 360) % 360;
    let cloudiness = clamp01(0.22 + 0.46 * hashDay(day + 17) + 0.12 * Math.sin(dayProgress * TAU + dayNoise * 5));
    let rain = 0, snow = 0, storm = 0, lightning = 0;
    if (!opts.weather) {
      wind = 0.32; gust = 0.04; windDirection = 110; cloudiness = 0.22;
    } else if (opts.preset === 'clear') {
      wind = 0.25; gust = 0.05; cloudiness = 0.08;
    } else if (opts.preset === 'cloudy') {
      wind = 0.42; gust = 0.12; cloudiness = 0.78;
    } else if (opts.preset === 'rain') {
      wind = 0.56; gust = 0.22; cloudiness = 0.94; rain = 0.72;
    } else if (opts.preset === 'storm') {
      wind = 0.84; gust = 0.82; cloudiness = 1.0; rain = 1.0; storm = 1.0;
      // Short, irregular flashes instead of a continuous sine-wave glow.
      // The cycle is deterministic for a given day, while the 100 ms storm
      // refresh below keeps the pulse visible to the 20–30 fps overlay.
      const stormSeconds = date.getTime() / 1000 + dayNoise * 37.0;
      const cycle = ((stormSeconds % 11.5) + 11.5) % 11.5;
      const firstFlash = Math.exp(-Math.pow((cycle - 1.15) / 0.095, 2));
      const secondFlash = dayNoise > 0.34 ? 0.72 * Math.exp(-Math.pow((cycle - 1.36) / 0.065, 2)) : 0;
      lightning = clamp01(Math.max(firstFlash, secondFlash));
    } else if (opts.preset === 'snow') {
      wind = 0.34; gust = 0.16; cloudiness = 0.82; snow = 0.86;
    } else {
      // Auto weather is deterministic and quiet: only sufficiently cloudy
      // periods produce precipitation, with winter snow based on local month.
      // A slow “front” moves through the day so the status can change while
      // the app is running instead of staying fixed until midnight.
      const front = 0.5 + 0.5 * Math.sin(dayProgress * TAU * 1.15 + dayNoise * 8.0);
      cloudiness = clamp01(cloudiness * 0.68 + front * 0.32);
      rain = clamp01((cloudiness - 0.68) * 3.1);
      const month = date.getMonth();
      snow = (month === 11 || month <= 1) ? rain * 0.78 : 0;
    }
    const remote = liveWeatherFor(opts);
    if (remote) {
      const code = Number(remote.weather_code) || 0;
      const precipitation = Number(remote.precipitation) || 0;
      const rainNow = Number(remote.rain) || 0;
      const showers = Number(remote.showers) || 0;
      const snowfall = Number(remote.snowfall) || 0;
      const remoteStorm = code >= 95 ? 1 : 0;
      wind = clamp01((Number(remote.wind_speed_10m) || 0) / 70);
      gust = clamp01((Number(remote.wind_gusts_10m) || 0) / 100);
      windDirection = ((Number(remote.wind_direction_10m) || 0) + 360) % 360;
      cloudiness = clamp01((Number(remote.cloud_cover) || 0) / 100);
      rain = clamp01((precipitation + rainNow + showers) / 4);
      snow = clamp01(snowfall / 3);
      storm = remoteStorm;
      lightning = remoteStorm ? clamp01(0.5 + 0.5 * Math.sin(date.getTime() / 1000 * 1.7 + opts.location.latitude)) : 0;
    }
    let weatherState = 'clear';
    if (!opts.weather) weatherState = 'disabled';
    else if (storm > 0.5) weatherState = 'storm';
    else if (snow > 0.18) weatherState = 'snow';
    else if (rain > 0.18) weatherState = 'rain';
    else if (cloudiness > 0.6) weatherState = 'cloudy';
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
      now: date.getTime(), day, dayProgress, hour: h, clockHour,
      timeEnabled: opts.time, weatherEnabled: opts.weather, weatherPreset: opts.preset,
      weatherSource: remote ? 'live' : 'local', location: opts.location,
      phase, daylight, dawn, dusk, golden, night,
      sunElevation, sunAzimuth, noon,
      light: Object.freeze(light),
      temperatureTint: warm,
      wind, gust, windDirection, cloudiness, rain, snow, storm, lightning, weatherState,
      // convenient normalized values for shader uniforms and theme code
      skyLight: lightStrength,
      isDay: daylight > 0.35,
    });
  }
  function currentEnvironment() {
    const now = Date.now();
    // Normal scenes refresh once per second. Storms refresh at 100 ms so a
    // short lightning pulse is not quantized into a slow, artificial blink.
    const refreshMs = environmentCache?.weatherPreset === 'storm' ? 100 : 1000;
    if (!environmentCache || now < environmentStamp || now - environmentStamp >= refreshMs) {
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
      targetFps = Number.isFinite(Number(next.fps)) ? Math.max(10, Math.min(60, Number(next.fps))) : 30;
      epochOffset = Date.now() - performance.now() - next.epoch;
      if (next.environment) api.setEnvironmentOptions(next.environment);
    },
    seconds(now, fallback) { return contract ? (now + epochOffset) / 1000 : (now - fallback) / 1000; },
    frameDue(now) {
      if (now < nextFrameAt) return false;
      const interval = 1000 / targetFps;
      nextFrameAt = !nextFrameAt || now - nextFrameAt > 100 ? now + interval : nextFrameAt + interval;
      return true;
    },
    resetFrame() { nextFrameAt = 0; },
    noteDraw() { draws++; },
    received(message) {
      if (message && message.environment) api.setEnvironmentOptions(message.environment);
      if (message && message.t === 'environment') api.setEnvironmentOptions(message);
      if (message.t === 'icons') iconsSeen = true;
      // Run after the page's handler has applied the new geometry/parameters.
      if (['icons', 'settings', 'reset', 'pause'].includes(message.t))
        queueMicrotask(() => { if (api.invalidate) api.invalidate(); });
    },
    get diagnostics() { return { draws, iconsSeen, errors: errors.slice() }; },
    environmentAt,
    get environment() { return currentEnvironment(); },
    refreshEnvironment() { environmentCache = null; return currentEnvironment(); },
    setEnvironmentOptions(options) {
      environmentOptions = normalizeEnvironmentOptions(options);
      environmentCache = null;
      return currentEnvironment();
    },
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
