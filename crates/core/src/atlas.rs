//! Procedurally generated 4x4 texture atlas (64x64 px, RGBA8).
//!
//! Every tile is painted from a fixed per-tile RNG seed so the atlas is
//! bit-identical across runs and platforms; nothing here reads files.

use crate::noise::value_noise2;
use crate::rng::Lcg;

/// Edge length in pixels of one tile.
pub const TILE_PX: u32 = 16;
/// Tiles per atlas row/column (the atlas is square).
pub const ATLAS_TILES: u32 = 4;
/// Full atlas edge length in pixels.
pub const ATLAS_PX: u32 = TILE_PX * ATLAS_TILES;

/// Grass block top face.
pub const T_GRASS_TOP: u8 = 0;
/// Grass block side face (dirt with grass fringe).
pub const T_GRASS_SIDE: u8 = 1;
/// Dirt.
pub const T_DIRT: u8 = 2;
/// Stone.
pub const T_STONE: u8 = 3;
/// Sand.
pub const T_SAND: u8 = 4;
/// Oak log side face (bark stripes).
pub const T_LOG_SIDE: u8 = 5;
/// Oak log top face (rings).
pub const T_LOG_TOP: u8 = 6;
/// Leaves (cutout: some pixels fully transparent).
pub const T_LEAVES: u8 = 7;
/// Planks (with seams).
pub const T_PLANK: u8 = 8;
/// Cobblestone (noise-driven stones).
pub const T_COBBLE: u8 = 9;
/// Glass (transparent interior, opaque frame).
pub const T_GLASS: u8 = 10;
/// Snow top face.
pub const T_SNOW_TOP: u8 = 11;
/// Snow side face (dirt with snow fringe).
pub const T_SNOW_SIDE: u8 = 12;
/// Bedrock (dark noise).
pub const T_BEDROCK: u8 = 13;
/// Water (translucent).
pub const T_WATER: u8 = 14;
/// Solid white, used for UI/outline quads.
pub const T_WHITE: u8 = 15;

/// A generated RGBA8 atlas image, row-major, 4 bytes per pixel.
pub struct Atlas {
    /// Pixel data of size `ATLAS_PX * ATLAS_PX * 4`.
    pub rgba: Vec<u8>,
}

fn vary(base: [f32; 3], amt: f32, rng: &mut Lcg) -> [u8; 4] {
    let f = 1.0 - amt / 2.0 + rng.next_f32() * amt;
    let c = |v: f32| (v * f).round().clamp(0.0, 255.0) as u8;
    [c(base[0]), c(base[1]), c(base[2]), 255]
}

fn scaled(c: [u8; 4], f: f32) -> [u8; 4] {
    [
        (f32::from(c[0]) * f).round().clamp(0.0, 255.0) as u8,
        (f32::from(c[1]) * f).round().clamp(0.0, 255.0) as u8,
        (f32::from(c[2]) * f).round().clamp(0.0, 255.0) as u8,
        c[3],
    ]
}

fn paint(tile: u8, x: u32, y: u32, rng: &mut Lcg) -> [u8; 4] {
    match tile {
        T_GRASS_TOP => vary([98.0, 160.0, 55.0], 0.28, rng),
        T_GRASS_SIDE => {
            if y < 3 || (y == 3 && rng.next_f32() < 0.6) || (y == 4 && rng.next_f32() < 0.15) {
                vary([98.0, 160.0, 55.0], 0.28, rng)
            } else {
                vary([134.0, 96.0, 67.0], 0.35, rng)
            }
        }
        T_DIRT => vary([134.0, 96.0, 67.0], 0.35, rng),
        T_STONE => {
            let c = vary([127.0, 127.0, 127.0], 0.18, rng);
            if rng.next_f32() < 0.08 {
                scaled(c, 0.78)
            } else {
                c
            }
        }
        T_SAND => vary([219.0, 207.0, 163.0], 0.14, rng),
        T_LOG_SIDE => {
            let s = if x % 4 < 2 { 0.82 } else { 1.05 };
            scaled(vary([107.0, 84.0, 51.0], 0.12, rng), s)
        }
        T_LOG_TOP => {
            let dx = (x as f32 - 7.5).abs();
            let dy = (y as f32 - 7.5).abs();
            let d = dx.max(dy);
            if d > 6.5 {
                vary([107.0, 84.0, 51.0], 0.15, rng)
            } else if (d as u32).is_multiple_of(2) {
                vary([178.0, 144.0, 90.0], 0.08, rng)
            } else {
                vary([130.0, 102.0, 60.0], 0.08, rng)
            }
        }
        T_LEAVES => {
            if rng.next_f32() < 0.16 {
                [0, 0, 0, 0]
            } else {
                vary([88.0, 138.0, 52.0], 0.3, rng)
            }
        }
        T_PLANK => {
            let seam =
                y.is_multiple_of(4) || ((x == 7 || x == 15) && (y / 4) % 2 == u32::from(x != 7));
            let amt = if seam { 0.02 } else { 0.12 };
            let c = vary([168.0, 133.0, 81.0], amt, rng);
            if seam { scaled(c, 0.6) } else { c }
        }
        T_COBBLE => {
            let n = value_noise2(x as f32 / 3.0, y as f32 / 3.0, 910);
            if n < 0.28 {
                vary([70.0, 70.0, 70.0], 0.15, rng)
            } else {
                let n2 = value_noise2(x as f32 / 2.0 + 40.0, y as f32 / 2.0 - 17.0, 911);
                scaled(vary([126.0, 126.0, 126.0], 0.12, rng), 0.72 + n2 * 0.45)
            }
        }
        T_GLASS => {
            let edge = x == 0 || y == 0 || x == TILE_PX - 1 || y == TILE_PX - 1;
            if edge {
                [214, 236, 240, 255]
            } else if (x + y == 20 && x > 8) || (x + y == 24 && x > 11) {
                [225, 242, 246, 255]
            } else {
                [180, 220, 230, 0]
            }
        }
        T_SNOW_TOP => vary([241.0, 246.0, 250.0], 0.06, rng),
        T_SNOW_SIDE => {
            if y < 4 || (y == 4 && rng.next_f32() < 0.5) {
                vary([241.0, 246.0, 250.0], 0.06, rng)
            } else {
                vary([134.0, 96.0, 67.0], 0.35, rng)
            }
        }
        T_BEDROCK => {
            let n = value_noise2(x as f32 / 2.5, y as f32 / 2.5, 915);
            let base = if n < 0.5 { 42.0 } else { 88.0 };
            vary([base, base, base], 0.25, rng)
        }
        T_WATER => {
            let mut c = vary([52.0, 110.0, 198.0], 0.12, rng);
            c[3] = 190;
            c
        }
        T_WHITE => [255, 255, 255, 255],
        _ => [255, 0, 255, 255],
    }
}

/// Paints the whole atlas deterministically.
///
/// The returned image is identical for every call on every platform; tiles
/// are laid out row-major starting at the top-left corner.
pub fn generate() -> Atlas {
    let mut rgba = vec![0u8; (ATLAS_PX * ATLAS_PX * 4) as usize];
    let last_tile = T_WHITE;
    for tile in 0..=last_tile {
        let col = u32::from(tile) % ATLAS_TILES;
        let row = u32::from(tile) / ATLAS_TILES;
        let mut rng = Lcg::new(u32::from(tile).wrapping_mul(7919).wrapping_add(101));
        for y in 0..TILE_PX {
            for x in 0..TILE_PX {
                let px = paint(tile, x, y, &mut rng);
                let dst = ((row * TILE_PX + y) * ATLAS_PX + col * TILE_PX + x) as usize * 4;
                rgba[dst..dst + 4].copy_from_slice(&px);
            }
        }
    }
    Atlas { rgba }
}

/// Maps an atlas-local UV `(u, v)` of `tile` into full-atlas UV space.
///
/// The result is inset by a small padding so bilinear sampling never bleeds
/// into neighboring tiles.
pub fn tile_uv(tile: u8, u: f32, v: f32) -> [f32; 2] {
    const PAD: f32 = 0.03125;
    let col = (u32::from(tile) % ATLAS_TILES) as f32;
    let row = (u32::from(tile) / ATLAS_TILES) as f32;
    let uu = (col + PAD + u * (1.0 - 2.0 * PAD)) / ATLAS_TILES as f32;
    let vv = (row + 1.0 - PAD - v * (1.0 - 2.0 * PAD)) / ATLAS_TILES as f32;
    [uu, vv]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atlas_is_deterministic_and_sized() {
        let a = generate();
        let b = generate();
        assert_eq!(a.rgba.len(), (ATLAS_PX * ATLAS_PX * 4) as usize);
        assert_eq!(a.rgba, b.rgba);
    }

    #[test]
    fn water_is_translucent_and_glass_interior_is_hollow() {
        let a = generate();
        let water_px = |tx: u32, ty: u32| {
            let i = (ty * ATLAS_PX + tx) as usize * 4;
            a.rgba[i + 3]
        };
        let water_col = (u32::from(T_WATER) % ATLAS_TILES) * TILE_PX;
        let water_row = (u32::from(T_WATER) / ATLAS_TILES) * TILE_PX;
        assert!(water_px(water_col + 8, water_row + 8) > 0);
        let glass_col = (u32::from(T_GLASS) % ATLAS_TILES) * TILE_PX;
        let glass_row = (u32::from(T_GLASS) / ATLAS_TILES) * TILE_PX;
        assert_eq!(water_px(glass_col + 4, glass_row + 4), 0);
    }

    #[test]
    fn tile_uv_maps_corners_inside_atlas() {
        let [u, v] = tile_uv(T_WATER, 0.0, 0.0);
        assert!((0.0..=1.0).contains(&u));
        assert!((0.0..=1.0).contains(&v));
        let [u2, v2] = tile_uv(T_WATER, 1.0, 1.0);
        assert!((0.0..=1.0).contains(&u2));
        assert!((0.0..=1.0).contains(&v2));
    }
}
