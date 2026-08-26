//! Chunked world storage: load/generate, edit, and block queries.

use std::collections::HashMap;

use crate::blocks::{AIR, BlockId, is_solid};
use crate::generation::settings::WorldGenSettings;
use crate::generation::{self, CHUNK_AXIS, ChunkData, GenCtx, Stage};

/// World vertical extent in blocks, re-exported from generation.
pub const HEIGHT: i32 = generation::CHUNK_HEIGHT;

/// One loaded 16x64x16 chunk plus its optional generated data.
pub struct Chunk {
    /// Chunk X coordinate in chunk space.
    pub cx: i32,
    /// Chunk Z coordinate in chunk space.
    pub cz: i32,
    data: Option<ChunkData>,
}

impl Chunk {
    /// Creates an empty chunk shell without generated data.
    pub const fn new(cx: i32, cz: i32) -> Self {
        Self { cx, cz, data: None }
    }

    /// Whether terrain data has been generated for this chunk.
    #[must_use]
    pub const fn has_data(&self) -> bool {
        self.data.is_some()
    }

    /// The generated block data, if any.
    #[must_use]
    pub const fn data(&self) -> Option<&ChunkData> {
        self.data.as_ref()
    }
}

/// The voxel world: an addressable map of [`Chunk`]s driven by a
/// generation pipeline.
///
/// Unloaded chunks read as air for rendering but act solid for physics,
/// so entities cannot fall through not-yet-generated ground.
pub struct World {
    gen_ctx: GenCtx,
    stages: Vec<Stage>,
    chunks: HashMap<(i32, i32), Chunk>,
}

impl World {
    /// A world using the default generation pipeline.
    pub fn new(seed: i32) -> Self {
        Self::with_stages(seed, generation::pipeline())
    }

    /// A world using a custom stage list (used by tests to simplify terrain).
    pub fn with_stages(seed: i32, stages: Vec<Stage>) -> Self {
        Self {
            gen_ctx: GenCtx {
                seed,
                settings: WorldGenSettings::DEFAULTS,
            },
            stages,
            chunks: HashMap::new(),
        }
    }

    /// The loaded chunk at `(cx, cz)`, if present.
    #[must_use]
    pub fn get_chunk(&self, cx: i32, cz: i32) -> Option<&Chunk> {
        self.chunks.get(&(cx, cz))
    }

    /// Generates data for the chunk if missing; existing data is kept.
    /// Returns whether generation ran.
    pub fn ensure_data(&mut self, cx: i32, cz: i32) -> bool {
        let has_data = self.get_chunk(cx, cz).is_some_and(Chunk::has_data);
        if has_data {
            return false;
        }
        let data = generation::generate(&self.gen_ctx, cx, cz, &self.stages);
        let chunk = self
            .chunks
            .entry((cx, cz))
            .or_insert_with(|| Chunk::new(cx, cz));
        chunk.data = Some(data);
        true
    }

    /// Generates every missing chunk within `radius` (Chebyshev distance) of
    /// `(cx, cz)` and returns how many were generated.
    pub fn ensure_around(&mut self, cx: i32, cz: i32, radius: i32) -> usize {
        let mut generated = 0;
        for z in (cz - radius)..=(cz + radius) {
            for x in (cx - radius)..=(cx + radius) {
                generated += usize::from(self.ensure_data(x, z));
            }
        }
        generated
    }

    /// Unloads the chunk at `(cx, cz)`, discarding any edits it contained.
    pub fn remove_chunk(&mut self, cx: i32, cz: i32) {
        self.chunks.remove(&(cx, cz));
    }

    /// Coordinates of every currently loaded chunk.
    pub fn loaded_chunks(&self) -> impl Iterator<Item = (i32, i32)> + '_ {
        self.chunks.keys().copied()
    }

    /// The block at world coordinates; outside the height range or in
    /// unloaded chunks this reads as air.
    #[must_use]
    pub fn get_block(&self, x: i32, y: i32, z: i32) -> BlockId {
        if !(0..HEIGHT).contains(&y) {
            return AIR;
        }
        self.chunks
            .get(&(x >> 4, z >> 4))
            .and_then(Chunk::data)
            .map_or(AIR, |d| d.get(x & 15, y, z & 15))
    }

    /// Reads the 18x66x18 neighborhood of chunk `(cx, cz)` into a flat
    /// view: one block border per side, cells outside loaded chunks or
    /// outside the height range read as air.
    #[must_use]
    pub(crate) fn padded_view(&self, cx: i32, cz: i32) -> PaddedChunk {
        let mut view = PaddedChunk {
            blocks: vec![AIR; PADDED_VOL],
        };
        for dz in -1_i32..=1 {
            for dx in -1_i32..=1 {
                let Some(data) = self.get_chunk(cx + dx, cz + dz).and_then(Chunk::data) else {
                    continue;
                };
                let (px0, nx, sx0) = pad_span(dx);
                let (pz0, nz, sz0) = pad_span(dz);
                for y in 0..HEIGHT {
                    for r in 0..nz {
                        let wz = pz0 + r as i32;
                        let lz = sz0 + r as i32;
                        let dst = padded_index(px0, y, wz);
                        if nx == CHUNK_AXIS as usize {
                            let src = generation::idx(0, y, lz);
                            view.blocks[dst..dst + nx]
                                .copy_from_slice(&data.blocks()[src..src + nx]);
                        } else {
                            view.blocks[dst] = data.blocks()[generation::idx(sx0, y, lz)];
                        }
                    }
                }
            }
        }
        view
    }

    /// Whether physics should treat the position as blocked.
    ///
    /// Unloaded chunks count as solid so players never fall into void.
    #[must_use]
    pub fn is_solid_at(&self, x: i32, y: i32, z: i32) -> bool {
        if !(0..HEIGHT).contains(&y) {
            return false;
        }
        self.chunks
            .get(&(x >> 4, z >> 4))
            .and_then(Chunk::data)
            .is_none_or(|d| is_solid(d.get(x & 15, y, z & 15)))
    }

    /// Places a block, generating the containing chunk first when needed.
    ///
    /// Returns the coordinates of every chunk whose mesh may have changed:
    /// the edited chunk plus bordering chunks when the edit touches an edge
    /// (neighbor faces and ambient occlusion can change).
    ///
    /// # Panics
    ///
    /// Panics if the chunk entry vanished after being ensured above, which
    /// cannot happen because no other code runs in between.
    #[must_use]
    pub fn set_block(&mut self, x: i32, y: i32, z: i32, id: BlockId) -> Vec<(i32, i32)> {
        if !(0..HEIGHT).contains(&y) {
            return Vec::new();
        }
        let cx = x >> 4;
        let cz = z >> 4;
        self.ensure_data(cx, cz);
        #[expect(
            clippy::expect_used,
            reason = "entry was created by ensure_data immediately above"
        )]
        let chunk = self.chunks.get_mut(&(cx, cz)).expect("just ensured");
        if let Some(d) = chunk.data.as_mut() {
            d.set(x & 15, y, z & 15, id);
        }
        let (lx, lz) = (x & 15, z & 15);
        let mut affected = vec![(cx, cz)];
        let push = |set: &mut Vec<(i32, i32)>, a: i32, b: i32| set.push((a, b));
        if lx == 0 {
            push(&mut affected, cx - 1, cz);
        }
        if lx == 15 {
            push(&mut affected, cx + 1, cz);
        }
        if lz == 0 {
            push(&mut affected, cx, cz - 1);
        }
        if lz == 15 {
            push(&mut affected, cx, cz + 1);
        }
        if lx == 0 && lz == 0 {
            push(&mut affected, cx - 1, cz - 1);
        }
        if lx == 0 && lz == 15 {
            push(&mut affected, cx - 1, cz + 1);
        }
        if lx == 15 && lz == 0 {
            push(&mut affected, cx + 1, cz - 1);
        }
        if lx == 15 && lz == 15 {
            push(&mut affected, cx + 1, cz + 1);
        }
        affected
    }
}

/// Flat 18x66x18 block neighborhood of one chunk: the chunk plus a
/// one-block border copied from its loaded neighbors.
pub(crate) struct PaddedChunk {
    /// Blocks ordered by [`padded_index`]; unfilled cells are air.
    blocks: Vec<BlockId>,
}

impl PaddedChunk {
    /// The block at chunk-relative coordinates; `x`/`z` in `-1..=16`
    /// and `y` in `-1..=64`.
    #[must_use]
    pub(crate) fn get(&self, x: i32, y: i32, z: i32) -> BlockId {
        self.blocks[padded_index(x, y, z)]
    }
}

/// One-block border around the chunk on X/Z.
const PADDED_AXIS: usize = CHUNK_AXIS as usize + 2;
/// One air layer below and above the chunk.
const PADDED_HEIGHT: usize = HEIGHT as usize + 2;
const PADDED_VOL: usize = PADDED_AXIS * PADDED_HEIGHT * PADDED_AXIS;

const fn padded_index(x: i32, y: i32, z: i32) -> usize {
    ((y + 1) as usize * PADDED_AXIS + (z + 1) as usize) * PADDED_AXIS + (x + 1) as usize
}

/// Destination start, length and source start for one neighbor offset:
/// `-1` copies the neighbor's last row into the border cell, `1` copies
/// its first row past the far edge.
const fn pad_span(offset: i32) -> (i32, usize, i32) {
    const SPANS: [(i32, usize, i32); 3] = [
        (-1, 1, CHUNK_AXIS - 1),
        (0, CHUNK_AXIS as usize, 0),
        (CHUNK_AXIS, 1, 0),
    ];
    SPANS[(offset + 1) as usize]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocks::{STONE, WATER};

    fn world() -> World {
        let mut w = World::new(7);
        w.ensure_data(0, 0);
        w
    }

    #[test]
    fn missing_chunks_block_movement_but_read_as_air() {
        let w = world();
        assert_eq!(w.get_block(100, 20, 100), AIR);
        assert!(w.is_solid_at(100, 20, 100));
    }

    #[test]
    fn set_block_updates_and_reports_border_neighbors() {
        let mut w = world();
        let affected = w.set_block(0, 10, 0, STONE);
        assert_eq!(w.get_block(0, 10, 0), STONE);
        assert!(affected.contains(&(0, 0)));
        assert!(affected.contains(&(-1, 0)));
        assert!(affected.contains(&(0, -1)));
        assert!(affected.contains(&(-1, -1)));

        let mid = w.set_block(8, 10, 8, STONE);
        assert_eq!(mid, vec![(0, 0)]);
    }

    #[test]
    fn water_is_not_solid_for_physics() {
        let mut w = world();
        let _ = w.set_block(5, 20, 5, WATER);
        assert!(!w.is_solid_at(5, 20, 5));
    }

    #[test]
    fn ensure_around_generates_the_square_once() {
        let mut w = World::new(7);
        assert_eq!(w.ensure_around(3, -2, 1), 9);
        assert_eq!(w.ensure_around(3, -2, 1), 0);
        assert!(w.get_chunk(4, -1).is_some_and(Chunk::has_data));
        assert_eq!(w.loaded_chunks().count(), 9);
    }

    #[test]
    fn edits_outside_height_are_rejected() {
        let mut w = world();
        assert!(w.set_block(0, -1, 0, STONE).is_empty());
        assert!(w.set_block(0, HEIGHT, 0, STONE).is_empty());
    }

    #[test]
    fn padded_view_matches_get_block_across_the_neighborhood() {
        let mut w = World::new(7);
        w.ensure_around(0, 0, 1);
        let v = w.padded_view(0, 0);
        for y in 0..HEIGHT {
            for z in -1..=CHUNK_AXIS {
                for x in -1..=CHUNK_AXIS {
                    assert_eq!(v.get(x, y, z), w.get_block(x, y, z));
                }
            }
        }
    }

    #[test]
    fn padded_view_reads_air_outside_loaded_chunks_and_height() {
        let mut w = World::new(7);
        w.ensure_data(0, 0);
        let v = w.padded_view(0, 0);
        assert_eq!(v.get(-1, 10, 8), AIR);
        assert_eq!(v.get(16, 10, 16), AIR);
        assert_eq!(v.get(5, -1, 5), AIR);
        assert_eq!(v.get(5, HEIGHT, 5), AIR);
    }
}
