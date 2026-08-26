//! Terrain shaping stage: heightfield, surface materials and column metadata.

use super::settings::WorldGenSettings;
use super::{CHUNK_AXIS, ChunkData, GenCtx, SEA_LEVEL};
use crate::blocks::{BEDROCK, DIRT, GRASS, SAND, SNOW, STONE};
use crate::noise::{fbm2, hash2};

/// Default world seed used by tests and debug tooling.
pub const SEED: i32 = 1337;

/// Per-column terrain facts derived once and shared by later stages.
pub struct ColumnInfo {
    /// Surface height: the y of the topmost solid block.
    pub h: i32,
    /// Whether an oak tree trunk grows on this column.
    pub tree: bool,
    /// Whether the surface is beach sand (low elevations).
    pub sandy: bool,
    /// Whether the surface is snow (high elevations).
    pub snowy: bool,
}

/// Computes terrain facts for one world column from two fbm layers:
/// a low-frequency continent field and a high-frequency detail field,
/// plus a squared mountain term.
///
/// The `seed` is mixed with per-feature salts so fields decorrelate;
/// `s` supplies the tuning values.
pub fn column_info(wx: i32, wz: i32, seed: i32, s: &WorldGenSettings) -> ColumnInfo {
    let c = fbm2(
        wx as f32 * s.continent_scale,
        wz as f32 * s.continent_scale,
        seed,
        3,
    );
    let d = fbm2(
        wx as f32 * s.detail_scale,
        wz as f32 * s.detail_scale,
        seed ^ s.detail_salt,
        3,
    );
    let mountain_n = ((c - s.mountain_start) / s.mountain_sharpness).clamp(0.0, 1.0);
    let mountains = mountain_n * mountain_n * s.mountain_amplitude;
    let mut h =
        (s.base_height + c * c * s.continent_amplitude + d * d * s.hill_amplitude + mountains)
            .floor() as i32;
    h = h.clamp(s.height_min, s.height_max);
    let tree = h > SEA_LEVEL + s.beach_band
        && h < s.snow_line
        && hash2(wx, wz, seed ^ s.tree_salt) < s.tree_chance;
    ColumnInfo {
        h,
        tree,
        sandy: h <= SEA_LEVEL + s.beach_band,
        snowy: h >= s.snow_line,
    }
}

/// Fills bedrock at y=0, then stone with a 3-block dirt/sand skin topped by
/// grass, sand or snow according to [`column_info`].
pub fn run(ctx: &GenCtx, cx: i32, cz: i32, data: &mut ChunkData) {
    let s = &ctx.settings;
    for lz in 0..CHUNK_AXIS {
        for lx in 0..CHUNK_AXIS {
            let wx = cx * CHUNK_AXIS + lx;
            let wz = cz * CHUNK_AXIS + lz;
            let info = column_info(wx, wz, ctx.seed, s);
            data.set(lx, 0, lz, BEDROCK);
            for y in 1..=info.h {
                let id = if y == info.h {
                    if info.sandy {
                        SAND
                    } else if info.snowy {
                        SNOW
                    } else {
                        GRASS
                    }
                } else if y >= info.h - 3 {
                    if info.sandy { SAND } else { DIRT }
                } else {
                    STONE
                };
                data.set(lx, y, lz, id);
            }
        }
    }
}
