//! Tree placement stage: scatters oak trees whose canopies may cross chunk borders.

use super::terrain::column_info;
use super::{CHUNK_AXIS, ChunkData, GenCtx};
use crate::blocks::{AIR, DIRT, LEAVES, LOG};
use crate::noise::hash2;

fn set_local(
    data: &mut ChunkData,
    lx: i32,
    y: i32,
    lz: i32,
    id: crate::blocks::BlockId,
    only_air: bool,
) {
    if only_air && data.get(lx, y, lz) != AIR {
        return;
    }
    data.set(lx, y, lz, id);
}

/// Places oak trees for every nearby column that [`column_info`] marked as
/// carrying one.
///
/// Scanning a margin beyond the chunk, set by `tree_scan_margin`, lets
/// canopies rooted in neighboring chunks finish inside this one.
pub fn run(ctx: &GenCtx, cx: i32, cz: i32, data: &mut ChunkData) {
    let s = &ctx.settings;
    let margin = s.tree_scan_margin;
    for tz in -margin..(CHUNK_AXIS + margin) {
        for tx in -margin..(CHUNK_AXIS + margin) {
            let wx = cx * CHUNK_AXIS + tx;
            let wz = cz * CHUNK_AXIS + tz;
            let info = column_info(wx, wz, ctx.seed, s);
            if !info.tree {
                continue;
            }
            let th = s.tree_min_h
                + (hash2(wx, wz, ctx.seed ^ s.tree_height_salt)
                    * (s.tree_max_h - s.tree_min_h + 1) as f32)
                    .floor() as i32;
            let base = info.h + 1;
            let top_y = base + th - 1;
            for i in 0..th {
                set_local(data, tx, base + i, tz, LOG, false);
            }
            set_local(data, tx, info.h, tz, DIRT, false);
            for dy in -2..=1 {
                let y = top_y + dy;
                let r: i32 = if dy <= -1 { 2 } else { 1 };
                for dx in -r..=r {
                    for dz in -r..=r {
                        if dy == 1 && (dx.abs() + dz.abs()) > 1 {
                            continue;
                        }
                        if dx.abs() == r
                            && dz.abs() == r
                            && dy <= 0
                            && hash2(wx * 31 + dx, wz * 31 + dz, ctx.seed ^ s.canopy_salt) < 0.5
                        {
                            continue;
                        }
                        if dx == 0 && dz == 0 && dy <= 0 {
                            continue;
                        }
                        set_local(data, tx + dx, y, tz + dz, LEAVES, true);
                    }
                }
            }
        }
    }
}
