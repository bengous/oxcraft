//! Staged chunk generation: terrain, caves, water, trees.
//!
//! A chunk is filled by running a list of [`Stage`]s in order over an empty
//! [`ChunkData`]; every stage reads/writes the same block volume and must
//! depend only on [`GenCtx`] plus chunk coordinates, keeping generation
//! deterministic and embarrassingly parallel-friendly.

pub mod caves;
pub mod settings;
pub mod terrain;
pub mod trees;
pub mod water;

/// Chunks are square on the X/Z plane; edge length in blocks.
pub const CHUNK_AXIS: i32 = 16;
/// Vertical extent of a chunk in blocks.
pub const CHUNK_HEIGHT: i32 = 64;
/// Number of block cells in one chunk.
pub const CHUNK_VOL: usize = (CHUNK_AXIS * CHUNK_AXIS * CHUNK_HEIGHT) as usize;
/// Water surface height; columns below it flood up to this level.
pub const SEA_LEVEL: i32 = 24;

/// Shared inputs for all generation stages of a world.
pub struct GenCtx {
    /// World seed; stages mix it with per-feature salts.
    pub seed: i32,
    /// Tuning values driving every stage.
    pub settings: settings::WorldGenSettings,
}

/// The 16x64x16 block storage of a single chunk.
///
/// Out-of-range accesses through [`ChunkData::get`] read as air and
/// [`ChunkData::set`] writes are dropped, so callers never need bounds
/// checks of their own.
pub struct ChunkData {
    blocks: [crate::blocks::BlockId; CHUNK_VOL],
}

impl ChunkData {
    /// An all-air chunk.
    pub const fn empty() -> Self {
        Self {
            blocks: [crate::blocks::AIR; CHUNK_VOL],
        }
    }

    /// Reads the block at local coordinates; out-of-bounds yields air.
    pub fn get(&self, lx: i32, y: i32, lz: i32) -> crate::blocks::BlockId {
        if !(0..CHUNK_AXIS).contains(&lx)
            || !(0..CHUNK_AXIS).contains(&lz)
            || !(0..CHUNK_HEIGHT).contains(&y)
        {
            return crate::blocks::AIR;
        }
        self.blocks[idx(lx, y, lz)]
    }

    /// Writes the block at local coordinates; out-of-bounds writes are ignored.
    pub fn set(&mut self, lx: i32, y: i32, lz: i32, id: crate::blocks::BlockId) {
        if !(0..CHUNK_AXIS).contains(&lx)
            || !(0..CHUNK_AXIS).contains(&lz)
            || !(0..CHUNK_HEIGHT).contains(&y)
        {
            return;
        }
        self.blocks[idx(lx, y, lz)] = id;
    }

    /// Direct access to the packed block array, ordered by [`idx`].
    pub const fn blocks(&self) -> &[crate::blocks::BlockId; CHUNK_VOL] {
        &self.blocks
    }
}

/// Packs local chunk coordinates `(lx, y, lz)` into an index into
/// [`ChunkData::blocks`]; arguments must be in range.
///
/// The bit shifts below assume [`CHUNK_AXIS`] is 16 and [`CHUNK_HEIGHT`] is
/// 64; the const assertion keeps the constants and the bit math in sync.
#[must_use]
pub const fn idx(lx: i32, y: i32, lz: i32) -> usize {
    const _: () = assert!(CHUNK_AXIS == 16 && CHUNK_HEIGHT == 64);
    ((y << 8) | (lz << 4) | lx) as usize
}

/// One named pass of the generation pipeline.
pub struct Stage {
    /// Human-readable stage name, used by tests and debugging.
    pub name: &'static str,
    /// Fills/edits `data` for chunk `(cx, cz)` under context `ctx`.
    pub run: fn(&GenCtx, i32, i32, &mut ChunkData),
}

/// The default generation pipeline: terrain, then caves, then water, then trees.
pub fn pipeline() -> Vec<Stage> {
    vec![
        Stage {
            name: "terrain",
            run: terrain::run,
        },
        Stage {
            name: "caves",
            run: caves::run,
        },
        Stage {
            name: "water",
            run: water::run,
        },
        Stage {
            name: "trees",
            run: trees::run,
        },
    ]
}

/// Generates one chunk by applying `stages` in order to an empty [`ChunkData`].
pub fn generate(ctx: &GenCtx, cx: i32, cz: i32, stages: &[Stage]) -> ChunkData {
    let mut data = ChunkData::empty();
    for stage in stages {
        (stage.run)(ctx, cx, cz, &mut data);
    }
    data
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocks::{self, BEDROCK, GRASS, RenderKind, SAND, WATER, def};

    fn ctx(seed: i32) -> GenCtx {
        GenCtx {
            seed,
            settings: settings::WorldGenSettings::DEFAULTS,
        }
    }

    #[test]
    fn generation_is_deterministic() {
        let ctx = ctx(1337);
        let stages = pipeline();
        let a = generate(&ctx, 0, 0, &stages);
        let b = generate(&ctx, 0, 0, &stages);
        assert_eq!(a.blocks(), b.blocks());
    }

    #[test]
    fn bedrock_covers_the_floor() {
        let ctx = ctx(1337);
        let data = generate(&ctx, 2, -3, &pipeline());
        for lz in 0..CHUNK_AXIS {
            for lx in 0..CHUNK_AXIS {
                assert_eq!(data.get(lx, 0, lz), BEDROCK);
            }
        }
    }

    #[test]
    fn water_fills_low_columns_up_to_sea_level() {
        let ctx = ctx(1337);
        let stages = pipeline();
        let mut found = false;
        for cz in -8..8 {
            for cx in -8..8 {
                let data = generate(&ctx, cx, cz, &stages);
                for lz in 0..CHUNK_AXIS {
                    for lx in 0..CHUNK_AXIS {
                        if data.get(lx, SEA_LEVEL, lz) == WATER {
                            found = true;
                            for y in SEA_LEVEL..CHUNK_HEIGHT {
                                assert_eq!(
                                    def(data.get(lx, y, lz)).render,
                                    if y <= SEA_LEVEL {
                                        RenderKind::Liquid
                                    } else {
                                        RenderKind::Air
                                    }
                                );
                            }
                        }
                    }
                }
            }
        }
        assert!(found, "no water generated in sampled chunks");
    }

    #[test]
    fn surface_is_walkable_ground_somewhere() {
        let ctx = ctx(1337);
        let stages = pipeline();
        let data = generate(&ctx, 0, 0, &stages);
        let mut surface_kinds = 0;
        for lz in 0..CHUNK_AXIS {
            for lx in 0..CHUNK_AXIS {
                for y in 1..CHUNK_HEIGHT {
                    let id = data.get(lx, y, lz);
                    if id == GRASS || id == SAND {
                        surface_kinds += 1;
                        break;
                    }
                }
            }
        }
        assert!(surface_kinds > 0);
        assert_eq!(blocks::def(GRASS).render, RenderKind::Opaque);
    }
}

#[cfg(test)]
mod height_distribution {
    use super::SEA_LEVEL;
    use super::settings::WorldGenSettings;
    use super::terrain::column_info;

    #[test]
    fn terrain_has_water_basins_and_peaks() {
        let mut min = i32::MAX;
        let mut max = i32::MIN;
        let mut under_sea = 0;
        let mut peaks = 0;
        for wz in -192..192 {
            for wx in -192..192 {
                let h = column_info(wx, wz, 1337, &WorldGenSettings::DEFAULTS).h;
                min = min.min(h);
                max = max.max(h);
                if h < SEA_LEVEL {
                    under_sea += 1;
                }
                if h >= 47 {
                    peaks += 1;
                }
            }
        }
        assert!(under_sea > 100, "no water basins: min={min}");
        assert!(peaks > 10, "no snowy peaks: max={max}");
    }
}

#[cfg(test)]
mod golden {
    use super::settings::WorldGenSettings;
    use super::terrain::column_info;
    use super::*;

    const GOLDEN_CHUNK_HASH_1337: u64 = 132_791_179_053_633_822;
    const GOLDEN_CHUNK_HASH_424242: u64 = 15_471_757_175_197_984_003;
    const GOLDEN_CHUNK_HASH_NEG_1337: u64 = 15_728_195_521_564_567_423;
    const GOLDEN_SPAWN_H_1337: i32 = 31;
    const GOLDEN_SPAWN_H_424242: i32 = 25;

    fn chunk_hash(cx: i32, cz: i32, seed: i32) -> u64 {
        let ctx = GenCtx {
            seed,
            settings: WorldGenSettings::DEFAULTS,
        };
        let mut digest = crate::digest::Fnv1a::new();
        digest.write(generate(&ctx, cx, cz, &pipeline()).blocks());
        digest.finish()
    }

    #[test]
    fn golden_chunk0_hash_is_pinned() {
        assert_eq!(chunk_hash(0, 0, 1337), GOLDEN_CHUNK_HASH_1337);
        assert_eq!(chunk_hash(0, 0, 424_242), GOLDEN_CHUNK_HASH_424242);
    }

    #[test]
    fn golden_negative_chunk_hash_is_pinned() {
        assert_eq!(chunk_hash(-2, -3, 1337), GOLDEN_CHUNK_HASH_NEG_1337);
    }

    #[test]
    fn golden_spawn_column_height_is_pinned() {
        assert_eq!(
            column_info(8, 8, 1337, &WorldGenSettings::DEFAULTS).h,
            GOLDEN_SPAWN_H_1337
        );
        assert_eq!(
            column_info(8, 8, 424_242, &WorldGenSettings::DEFAULTS).h,
            GOLDEN_SPAWN_H_424242
        );
    }
}
