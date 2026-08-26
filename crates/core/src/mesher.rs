//! Greedy-per-face chunk mesher with per-vertex ambient occlusion.
//!
//! Produces separate opaque and water geometry for a 16x64x16 column by
//! emitting only faces adjacent to non-hiding blocks.

use crate::atlas::tile_uv;
use crate::blocks::{AIR, RenderKind, def, is_opaque};
use crate::generation::{CHUNK_AXIS, CHUNK_HEIGHT};
use crate::world::{PaddedChunk, World};

/// One mesh vertex: cube-corner position, atlas UV and packed light term.
#[derive(Clone, Copy)]
pub struct Vertex {
    /// World-space corner position.
    pub pos: [f32; 3],
    /// Atlas texture coordinate.
    pub uv: [f32; 2],
    /// Face shade times ambient-occlusion factor.
    pub light: f32,
}

/// Raw triangle soup for one render pass.
#[derive(Default)]
pub struct Geometry {
    /// Vertices referenced by [`Geometry::indices`].
    pub vertices: Vec<Vertex>,
    /// Triangle index list into [`Geometry::vertices`].
    pub indices: Vec<u32>,
}

impl Geometry {
    /// Whether nothing would be drawn.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }
}

/// The complete drawable output for one chunk.
#[derive(Default)]
pub struct ChunkMesh {
    /// Solid + cutout geometry.
    pub opaque: Geometry,
    /// Translucent water geometry.
    pub water: Geometry,
}

/// One cube face: outward normal, corners in counter-clockwise order seen
/// from outside, and the directional shade the mesher applies.
pub struct FaceDef {
    /// Outward unit normal.
    pub dir: [i32; 3],
    /// Unit-cube corners, matching [`FACE_UV`] index by index.
    pub corners: [[f32; 3]; 4],
    /// Directional light factor.
    pub shade: f32,
}

/// The six cube faces in [`crate::blocks::BlockDef::tiles`] order:
/// +X, -X, +Y, -Y, +Z, -Z.
pub const FACES: [FaceDef; 6] = [
    FaceDef {
        dir: [1, 0, 0],
        corners: [
            [1.0, 0.0, 1.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [1.0, 1.0, 1.0],
        ],
        shade: 0.68,
    },
    FaceDef {
        dir: [-1, 0, 0],
        corners: [
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.0, 1.0, 1.0],
            [0.0, 1.0, 0.0],
        ],
        shade: 0.68,
    },
    FaceDef {
        dir: [0, 1, 0],
        corners: [
            [0.0, 1.0, 1.0],
            [1.0, 1.0, 1.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
        ],
        shade: 1.0,
    },
    FaceDef {
        dir: [0, -1, 0],
        corners: [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 0.0, 1.0],
            [0.0, 0.0, 1.0],
        ],
        shade: 0.55,
    },
    FaceDef {
        dir: [0, 0, 1],
        corners: [
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
            [0.0, 1.0, 1.0],
        ],
        shade: 0.84,
    },
    FaceDef {
        dir: [0, 0, -1],
        corners: [
            [1.0, 0.0, 0.0],
            [0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [1.0, 1.0, 0.0],
        ],
        shade: 0.84,
    },
];

/// Tile-local UV of each corner in [`FaceDef::corners`].
pub const FACE_UV: [[f32; 2]; 4] = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
const AO_LUT: [f32; 4] = [0.45, 0.66, 0.85, 1.0];

fn push_quad(geo: &mut Geometry, corners: &[[f32; 3]; 4], tile: u8, lights: [f32; 4], flip: bool) {
    let base = geo.vertices.len() as u32;
    for k in 0..4 {
        let [u, v] = tile_uv(tile, FACE_UV[k][0], FACE_UV[k][1]);
        geo.vertices.push(Vertex {
            pos: corners[k],
            uv: [u, v],
            light: lights[k],
        });
    }
    if flip {
        geo.indices
            .extend_from_slice(&[base + 1, base + 2, base + 3, base + 1, base + 3, base]);
    } else {
        geo.indices
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
}

fn ao_level(view: &PaddedChunk, cells: &[[i32; 3]; 3]) -> u32 {
    let s1 = u32::from(def(view.get(cells[0][0], cells[0][1], cells[0][2])).ao_cast);
    let s2 = u32::from(def(view.get(cells[1][0], cells[1][1], cells[1][2])).ao_cast);
    let sc = u32::from(def(view.get(cells[2][0], cells[2][1], cells[2][2])).ao_cast);
    if s1 == 1 && s2 == 1 {
        0
    } else {
        3 - (s1 + s2 + sc)
    }
}

#[expect(
    clippy::float_cmp,
    reason = "corner coordinates hold exact 0.0/1.0 constants, never computed floats"
)]
fn corner_sign(v: f32) -> i32 {
    if v == 1.0 { 1 } else { -1 }
}

/// Builds the mesh for chunk `(cx, cz)` from a one-block padded snapshot of
/// the world, so cross-chunk faces are decided without per-block lookups.
///
/// Water faces are emitted only against air; opaque/cutout faces are skipped
/// when the neighbor is opaque or the same block kind (glass-to-glass).
#[must_use]
pub fn build_mesh(world: &World, cx: i32, cz: i32) -> ChunkMesh {
    let mut mesh = ChunkMesh::default();
    let ox = cx * CHUNK_AXIS;
    let oz = cz * CHUNK_AXIS;
    let view = world.padded_view(cx, cz);

    for y in 0..CHUNK_HEIGHT {
        for z in 0..CHUNK_AXIS {
            for x in 0..CHUNK_AXIS {
                let id = view.get(x, y, z);
                if id == AIR {
                    continue;
                }
                let block_def = def(id);

                for face in &FACES {
                    let [dx, dy, dz] = face.dir;
                    let nb = view.get(x + dx, y + dy, z + dz);
                    let draw = match block_def.render {
                        RenderKind::Liquid => nb == AIR,
                        RenderKind::Opaque | RenderKind::Cutout => {
                            nb == AIR || (!is_opaque(nb) && nb != id)
                        }
                        RenderKind::Air => false,
                    };
                    if !draw {
                        continue;
                    }

                    let axis = if dx != 0 {
                        0
                    } else if dy != 0 {
                        1
                    } else {
                        2
                    };
                    let ta = (axis + 1) % 3;
                    let tb = (axis + 2) % 3;
                    let base_cell = [x + dx, y + dy, z + dz];

                    let mut corners = [[0.0f32; 3]; 4];
                    let mut lights = [0.0f32; 4];
                    let mut ao_levels = [0u32; 4];
                    for k in 0..4 {
                        let c = face.corners[k];
                        corners[k] = [
                            (ox + x) as f32 + c[0],
                            y as f32 + c[1],
                            (oz + z) as f32 + c[2],
                        ];
                        let sa = corner_sign(c[ta]);
                        let sb = corner_sign(c[tb]);
                        let mut o1 = base_cell;
                        let mut o2 = base_cell;
                        let mut oc = base_cell;
                        o1[ta] += sa;
                        o2[tb] += sb;
                        oc[ta] += sa;
                        oc[tb] += sb;
                        let level = if block_def.render == RenderKind::Liquid {
                            3
                        } else {
                            ao_level(&view, &[o1, o2, oc])
                        };
                        ao_levels[k] = level;
                        lights[k] = face.shade * AO_LUT[level as usize];
                    }
                    let flip = ao_levels[0] + ao_levels[2] < ao_levels[1] + ao_levels[3];
                    let geo = match block_def.render {
                        RenderKind::Liquid => &mut mesh.water,
                        _ => &mut mesh.opaque,
                    };
                    push_quad(
                        geo,
                        &corners,
                        block_def.tiles[axis_to_face_index(axis, dx, dy, dz)],
                        lights,
                        flip,
                    );
                }
            }
        }
    }
    mesh
}

const fn axis_to_face_index(axis: usize, dx: i32, dy: i32, dz: i32) -> usize {
    let positive = match axis {
        0 => dx > 0,
        1 => dy > 0,
        _ => dz > 0,
    };
    match (axis, positive) {
        (0, true) => 0,
        (0, false) => 1,
        (1, true) => 2,
        (1, false) => 3,
        (_, true) => 4,
        (_, false) => 5,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocks::STONE;

    fn single_block_world() -> World {
        let mut w = World::new(5);
        w.ensure_data(0, 0);
        for y in 0..CHUNK_HEIGHT {
            for z in 0..CHUNK_AXIS {
                for x in 0..CHUNK_AXIS {
                    let _ = w.set_block(x, y, z, crate::blocks::AIR);
                }
            }
        }
        let _ = w.set_block(8, 10, 8, STONE);
        w
    }

    #[test]
    fn isolated_cube_emits_six_faces() {
        let world = single_block_world();
        let mesh = build_mesh(&world, 0, 0);
        assert_eq!(mesh.opaque.vertices.len(), 24);
        assert_eq!(mesh.opaque.indices.len(), 36);
        assert!(mesh.water.is_empty());
    }

    #[test]
    fn hidden_faces_are_culled_between_solid_blocks() {
        let mut world = single_block_world();
        let _ = world.set_block(9, 10, 8, STONE);
        // two adjacent cubes share one face: 6 + 6 - 2 = 10 faces
        let mesh = build_mesh(&world, 0, 0);
        assert_eq!(mesh.opaque.vertices.len(), 40);
        assert_eq!(mesh.opaque.indices.len(), 60);
    }

    const GOLDEN_MESH_HASH_1337: u64 = 6_671_654_092_181_913_918;

    fn mesh_digest(mesh: &ChunkMesh) -> u64 {
        let mut digest = crate::digest::Fnv1a::new();
        for geo in [&mesh.opaque, &mesh.water] {
            for v in &geo.vertices {
                for value in v.pos.into_iter().chain(v.uv).chain([v.light]) {
                    digest.write_f32(value);
                }
            }
            for index in &geo.indices {
                digest.write(&index.to_le_bytes());
            }
        }
        digest.finish()
    }

    #[test]
    fn golden_chunk0_mesh_hash_is_pinned() {
        let mut w = World::new(1337);
        for cz in -1..=1 {
            for cx in -1..=1 {
                w.ensure_data(cx, cz);
            }
        }
        let mesh = build_mesh(&w, 0, 0);
        assert!(!mesh.opaque.is_empty());
        assert_eq!(mesh_digest(&mesh), GOLDEN_MESH_HASH_1337);
    }

    #[test]
    fn empty_chunk_produces_no_geometry() {
        let mut w = World::new(5);
        w.ensure_data(0, 0);
        for y in 0..CHUNK_HEIGHT {
            for z in 0..CHUNK_AXIS {
                for x in 0..CHUNK_AXIS {
                    let _ = w.set_block(x, y, z, crate::blocks::AIR);
                }
            }
        }
        let mesh = build_mesh(&w, 0, 0);
        assert!(mesh.opaque.is_empty());
        assert!(mesh.water.is_empty());
    }
}
