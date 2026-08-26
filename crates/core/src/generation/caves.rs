//! Cave carving stage: removes blocks where 3D fbm noise sits in a thin band.

use super::{CHUNK_AXIS, CHUNK_HEIGHT, ChunkData, GenCtx};
use crate::blocks::AIR;
use crate::noise::fbm3;

/// Hollows out caves by deleting stone where the cave noise field crosses
/// its mid-band, never touching the layers below `cave_min_y` or the
/// `cave_skin`-block skin under each column's surface.
pub fn run(ctx: &GenCtx, cx: i32, cz: i32, data: &mut ChunkData) {
    let s = &ctx.settings;
    for lz in 0..CHUNK_AXIS {
        for lx in 0..CHUNK_AXIS {
            let wx = cx * CHUNK_AXIS + lx;
            let wz = cz * CHUNK_AXIS + lz;
            let mut h = 0;
            for y in (s.cave_min_y..CHUNK_HEIGHT).rev() {
                if data.get(lx, y, lz) != AIR {
                    h = y;
                    break;
                }
            }
            let top = (h - s.cave_skin).min(s.cave_top_cap);
            for y in s.cave_min_y..=top {
                let n = fbm3(
                    wx as f32 * s.cave_scale_xz,
                    y as f32 * s.cave_scale_y,
                    wz as f32 * s.cave_scale_xz,
                    ctx.seed ^ s.cave_salt,
                    2,
                );
                if (n - 0.5).abs() < s.cave_band {
                    data.set(lx, y, lz, AIR);
                }
            }
        }
    }
}
