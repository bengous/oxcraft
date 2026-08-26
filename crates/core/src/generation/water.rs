//! Water flooding stage: fills columns below sea level up to [`SEA_LEVEL`].

use super::terrain::column_info;
use super::{CHUNK_AXIS, ChunkData, GenCtx, SEA_LEVEL};
use crate::blocks::WATER;

/// Floods every column whose terrain surface lies below [`SEA_LEVEL`] with
/// water from just above the surface up to and including the sea level.
pub fn run(ctx: &GenCtx, cx: i32, cz: i32, data: &mut ChunkData) {
    for lz in 0..CHUNK_AXIS {
        for lx in 0..CHUNK_AXIS {
            let wx = cx * CHUNK_AXIS + lx;
            let wz = cz * CHUNK_AXIS + lz;
            let info = column_info(wx, wz, ctx.seed, &ctx.settings);
            if info.h >= SEA_LEVEL {
                continue;
            }
            for y in (info.h + 1)..=SEA_LEVEL {
                data.set(lx, y, lz, WATER);
            }
        }
    }
}
