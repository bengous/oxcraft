//! Procedurally painted break-progress overlay: one alpha tile per stage,
//! laid side by side in a strip, painted from a fixed seed so every run
//! and platform gets the same pixels.

use crate::atlas::TILE_PX;
use crate::rng::Lcg;

/// Number of break stages painted side by side.
pub const STAGES: u32 = 10;
/// Strip width in pixels: one [`TILE_PX`] square per stage.
pub const STRIP_PX: u32 = STAGES * TILE_PX;

const WALKS: u32 = 12;
const SEED: u32 = 0xC5AC;

/// Alpha-only crack strip, row-major, `STRIP_PX` x `TILE_PX`; stage `s`
/// occupies columns `s * TILE_PX .. (s + 1) * TILE_PX`.
pub struct CrackStrip {
    /// One alpha byte per pixel.
    pub alpha: Vec<u8>,
}

struct Walk {
    steps: Vec<(u32, u32)>,
}

fn walk(rng: &mut Lcg) -> Walk {
    let angle = rng.next_f32() * std::f32::consts::TAU;
    let (dy, dx) = angle.sin_cos();
    let mut x = (TILE_PX / 2) as f32;
    let mut y = (TILE_PX / 2) as f32;
    let mut steps = Vec::new();
    for _ in 0..TILE_PX {
        let sideways = rng.next_f32() < 0.35;
        let (sx, sy) = if sideways { (-dy, dx) } else { (dx, dy) };
        x += sx;
        y += sy;
        if x < 0.0 || y < 0.0 || x >= TILE_PX as f32 || y >= TILE_PX as f32 {
            break;
        }
        steps.push((x as u32, y as u32));
    }
    Walk { steps }
}

/// Paints the strip: stage `s` draws the first `3 + s` walks, each cut to
/// its first `3 + s` steps, so every stage contains the previous one.
pub fn generate() -> CrackStrip {
    let mut rng = Lcg::new(SEED);
    let walks: Vec<Walk> = (0..WALKS).map(|_| walk(&mut rng)).collect();
    let mut alpha = vec![0u8; (STRIP_PX * TILE_PX) as usize];
    let center = TILE_PX / 2;
    for stage in 0..STAGES {
        let reach = (3 + stage) as usize;
        let value = (150 + 10 * stage) as u8;
        let mut set = |x: u32, y: u32| {
            alpha[(y * STRIP_PX + stage * TILE_PX + x) as usize] = value;
        };
        set(center, center);
        for w in walks.iter().take(reach) {
            for (x, y) in w.steps.iter().take(reach) {
                set(*x, *y);
            }
        }
    }
    CrackStrip { alpha }
}

impl CrackStrip {
    /// Alpha of pixel `(x, y)` inside stage `stage`.
    #[must_use]
    pub fn at(&self, stage: u32, x: u32, y: u32) -> u8 {
        self.alpha[(y * STRIP_PX + stage * TILE_PX + x) as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn count(strip: &CrackStrip, stage: u32) -> usize {
        (0..TILE_PX)
            .flat_map(|y| (0..TILE_PX).map(move |x| (x, y)))
            .filter(|(x, y)| strip.at(stage, *x, *y) > 0)
            .count()
    }

    #[test]
    fn strip_is_deterministic_and_sized() {
        let a = generate();
        let b = generate();
        assert_eq!(a.alpha.len(), (STRIP_PX * TILE_PX) as usize);
        assert_eq!(a.alpha, b.alpha);
    }

    #[test]
    fn stages_only_add_pixels() {
        let strip = generate();
        for stage in 1..STAGES {
            for y in 0..TILE_PX {
                for x in 0..TILE_PX {
                    assert!(strip.at(stage, x, y) >= strip.at(stage - 1, x, y));
                }
            }
            assert!(count(&strip, stage) >= count(&strip, stage - 1));
        }
        assert!(count(&strip, STAGES - 1) > count(&strip, 0));
    }

    #[test]
    fn cracks_start_at_the_center() {
        let strip = generate();
        let c = TILE_PX / 2;
        assert!(strip.at(0, c, c) > 0);
    }

    #[test]
    fn last_stage_reaches_an_edge() {
        let strip = generate();
        let last = STAGES - 1;
        let edge = (0..TILE_PX).any(|i| {
            strip.at(last, i, 0) > 0
                || strip.at(last, i, TILE_PX - 1) > 0
                || strip.at(last, 0, i) > 0
                || strip.at(last, TILE_PX - 1, i) > 0
        });
        assert!(edge);
    }
}
