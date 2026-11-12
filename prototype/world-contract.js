/* Shared renderer contract v1. All host rectangles are physical screen pixels.
   Panorama is a single world surface; local = screen - virtual-desktop origin.
   Mirror surfaces share epoch and seed, while keeping their own aspect ratio. */
(function (root) {
  'use strict';
  let contract = null, epochOffset = 0, randomState = 104729;
  const api = {
    configure(next) {
      if (!next || next.version !== 1) return;
      contract = next;
      epochOffset = Date.now() - performance.now() - next.epoch;
    },
    seconds(now, fallback) { return contract ? (now + epochOffset) / 1000 : (now - fallback) / 1000; },
    seed(value) { randomState = value >>> 0; },
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
  if (typeof module !== 'undefined') module.exports = api;
})(globalThis);
