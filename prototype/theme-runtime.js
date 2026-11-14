/* Compatibility bridge for the GPL Deskland-VN 1.0.2 browser themes.
   The host still owns monitor routing and the single Panorama surface. */
(() => {
  'use strict';
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
