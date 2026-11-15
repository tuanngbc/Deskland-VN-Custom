//! Deterministic palm placement on sand that is actually visible on an output.
use crate::engine::{IconBox, Rng};

/// Scene coordinates, top-left origin: x, y, width, height.
pub type Viewport = [f32; 4];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PalmPlacement {
    pub x: f32,
    pub y: f32,
    pub scale: f32,
    pub variant: f32,
    pub mirror: f32,
}

/// Test coverage by the union, including trees straddling an adjacent seam.
/// A bounding desktop rectangle alone incorrectly includes gaps between outputs.
pub fn covered(rect: Viewport, outputs: &[Viewport]) -> bool {
    let [x, y, w, h] = rect;
    let mut cuts = vec![x, x + w];
    for &[ox, _, ow, _] in outputs {
        if ox > x && ox < x + w { cuts.push(ox); }
        if ox + ow > x && ox + ow < x + w { cuts.push(ox + ow); }
    }
    cuts.sort_by(f32::total_cmp);
    for strip in cuts.windows(2) {
        if strip[1] - strip[0] < 0.001 { continue; }
        let mid = (strip[0] + strip[1]) * 0.5;
        let mut intervals: Vec<(f32, f32)> = outputs.iter()
            .filter(|r| mid >= r[0] && mid < r[0] + r[2])
            .map(|r| (r[1], r[1] + r[3])).collect();
        intervals.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut bottom = y;
        for (top, end) in intervals {
            if top > bottom + 0.001 { break; }
            bottom = bottom.max(end);
        }
        if bottom < y + h - 0.001 { return false; }
    }
    true
}

pub fn tree_bounds(p: &PalmPlacement) -> Viewport {
    // Full atlas quad plus sway margin; includes all five original silhouettes.
    [p.x - 114.0 * p.scale, p.y - 164.0 * p.scale,
     228.0 * p.scale, 260.0 * p.scale]
}

fn clear(icons: &[IconBox], x: f32, y: f32, radius: f32) -> bool {
    icons.iter().all(|i| {
        let dx = (i.x - 7.0 - x).max(0.0).max(x - i.x - i.w - 7.0);
        let dy = (i.y - 7.0 - y).max(0.0).max(y - i.y - i.h - 7.0);
        dx.hypot(dy) > radius
    })
}

fn island_envelope(sample: &impl Fn(f32, f32) -> f32, p: &PalmPlacement) -> bool {
    // Verify trunk, crown and both sides of the silhouette are on the sand.
    // The old single-point test allowed a trunk on the shore with the crown
    // projected far into the sea.
    let points = [
        (p.x, p.y),
        (p.x - 24.0 * p.scale, p.y - 55.0 * p.scale),
        (p.x + 24.0 * p.scale, p.y - 55.0 * p.scale),
        (p.x - 55.0 * p.scale, p.y - 105.0 * p.scale),
        (p.x + 55.0 * p.scale, p.y - 105.0 * p.scale),
        (p.x - 70.0 * p.scale, p.y - 18.0 * p.scale),
        (p.x + 70.0 * p.scale, p.y - 18.0 * p.scale),
    ];
    points.into_iter().all(|(x, y)| sample(x, y) < 8.0)
}

pub fn choose(w: f32, h: f32, seed: i32, want: usize, outputs: &[Viewport],
              icons: &[IconBox], sample: impl Fn(f32, f32) -> f32) -> Vec<PalmPlacement> {
    let mut chosen: Vec<PalmPlacement> = Vec::new();
    let mut rng = Rng::new(seed as u64 ^ 0x41fc72);
    let crowns = [(9.0, -84.0, 80.0), (18.0, -80.0, 84.0),
                  (27.0, -82.0, 84.0), (40.0, -78.0, 78.0), (-45.0, -65.0, 70.0)];
    let mut order = [0usize, 1, 2, 3, 4];
    for i in (1..order.len()).rev() {
        let j = (rng.next() * (i + 1) as f32) as usize;
        order.swap(i, j);
    }
    for n in 0..want.min(10) {
        let variant = order[n % 5];
        let scale = 0.62 * 0.80 * if variant == 4 { 1.35 } else { 1.0 };
        let (cx, cy, radius) = crowns[variant];
        let mut selected = None;
        // Preserve the original shore band first. Only relax spacing/shore if
        // necessary, never visibility or icon clearance, and never plant in water.
        for (lo, hi, spacing) in [(-58.0, -25.0, 75.0), (-90.0, -8.0, 55.0), (-180.0, -2.0, 32.0)] {
            let mut count = 0u32;
            let ox = rng.next() * 13.0;
            let oy = rng.next() * 13.0;
            for iy in (0..h as i32).step_by(13) {
                let y = iy as f32 + oy;
                for ix in (0..w as i32).step_by(13) {
                    let x = ix as f32 + ox;
                    let shore = sample(x, y);
                    if shore < lo || shore > hi { continue; }
                    if chosen.iter().any(|p| (p.x-x).hypot(p.y-y) < spacing) { continue; }
                    let sea_x = sample(x+8.0, y) - sample(x-8.0, y);
                    let mirror = if (variant == 1 || variant == 4) && sea_x * cx < 0.0 { -1.0 } else { 1.0 };
                    let p = PalmPlacement { x, y, scale, variant: variant as f32, mirror };
                    if !covered(tree_bounds(&p), outputs) { continue; }
                    if !island_envelope(&sample, &p) { continue; }
                    if !clear(icons, x, y-16.0*scale, 10.0*scale)
                        || !clear(icons, x+cx*scale*mirror, y+cy*scale, radius*scale) { continue; }
                    count += 1;
                    // Reservoir sampling over the entire visible sand, like JS.
                    if rng.next() < 1.0 / count as f32 { selected = Some(p); }
                }
            }
            if selected.is_some() { break; }
        }
        if let Some(p) = selected { chosen.push(p); }
    }
    chosen
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{Field, Noise, icon_rects};

    #[test]
    fn staggered_outputs_reject_gaps_but_allow_seams() {
        let outputs = [[0.0, 0.0, 1080.0, 1920.0], [1080.0, 685.0, 2560.0, 1080.0]];
        assert!(!covered([1200.0, 300.0, 100.0, 100.0], &outputs));
        assert!(!covered([1040.0, 640.0, 100.0, 100.0], &outputs));
        assert!(covered([1040.0, 700.0, 100.0, 100.0], &outputs));
        assert!(!covered([1200.0, 1740.0, 100.0, 100.0], &outputs));
    }

    #[test]
    fn eight_visible_palms_with_icons_on_primary_staggered_desktop() {
        let outputs = [[0.0, 0.0, 1080.0, 1920.0], [1080.0, 685.0, 2560.0, 1080.0]];
        // Primary icons are offset by the negative origin of the portrait screen.
        let icons: Vec<_> = (0..10).map(|i| IconBox {
            x: 1092.0 + (i / 7) as f32 * 86.0, y: 699.0 + (i % 7) as f32 * 105.0,
            w: 74.0, h: 86.0,
        }).collect();
        check_layout(3640.0, 1920.0, &outputs, &icons);
    }

    #[test]
    fn single_output_layout_still_places_eight_visible_palms() {
        let outputs = [[0.0, 0.0, 2560.0, 1080.0]];
        let icons: Vec<_> = (0..8).map(|i| IconBox {
            x: 12.0, y: 12.0 + i as f32 * 105.0, w: 74.0, h: 86.0,
        }).collect();
        check_layout(2560.0, 1080.0, &outputs, &icons);
    }

    fn check_layout(w: f32, h: f32, outputs: &[Viewport], icons: &[IconBox]) {
        let field = Field::new(w, h, &Noise { seed: 833374 });
        let sand = field.compute(&icon_rects(icons), 94.0, 37.0);
        let sample = |x, y| field.sample(&sand, x, y);
        let palms = choose(w, h, 833374, 8, outputs, icons, sample);
        assert_eq!(palms.len(), 8, "requested trees must be visible, not merely counted in the bounding canvas");
        for p in &palms {
            assert!(covered(tree_bounds(p), outputs), "hidden/cropped palm: {p:?}");
            assert!(sample(p.x, p.y) < -2.0, "palm in water: {p:?}");
        }
        assert_eq!(palms, choose(w, h, 833374, 8, outputs, icons, sample));
    }
}
