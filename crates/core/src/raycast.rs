//! Voxel DDA raycasting against the world grid.

use crate::blocks::def;
use crate::world::World;

/// A solid block hit by [`raycast`].
#[derive(Debug)]
pub struct RayHit {
    /// World X of the hit block.
    pub x: i32,
    /// World Y of the hit block.
    pub y: i32,
    /// World Z of the hit block.
    pub z: i32,
    /// The block kind encountered.
    pub block: crate::blocks::BlockId,
    /// Outward normal of the struck face.
    pub face: [i32; 3],
    /// Distance from origin to the crossing point in world units.
    pub dist: f32,
}

fn intbound(s: f32, ds: f32) -> f32 {
    if ds == 0.0 {
        return f32::INFINITY;
    }
    if ds > 0.0 {
        (s.floor() + 1.0 - s) / ds
    } else {
        (s - s.floor()) / -ds
    }
}

/// Steps cell-by-cell from `origin` along `dir` up to `max_dist` and reports
/// the first solid block crossed.
///
/// This is the classic Amanatides & Woo grid traversal: the float loop bound
/// below is intentional and terminates because each axis step advances `t`
/// by a strictly positive delta (or the axis is disabled via infinity).
#[expect(
    clippy::while_float,
    reason = "DDA traversal advances t by positive deltas; the float bound is the algorithm"
)]
#[must_use]
pub fn raycast(world: &World, origin: [f32; 3], dir: [f32; 3], max_dist: f32) -> Option<RayHit> {
    let mut x = origin[0].floor() as i32;
    let mut y = origin[1].floor() as i32;
    let mut z = origin[2].floor() as i32;
    let step_x = if dir[0] > 0.0 { 1 } else { -1 };
    let step_y = if dir[1] > 0.0 { 1 } else { -1 };
    let step_z = if dir[2] > 0.0 { 1 } else { -1 };
    let mut t_max = [
        intbound(origin[0], dir[0]),
        intbound(origin[1], dir[1]),
        intbound(origin[2], dir[2]),
    ];
    let t_delta = [
        if dir[0] == 0.0 {
            f32::INFINITY
        } else {
            (1.0 / dir[0]).abs()
        },
        if dir[1] == 0.0 {
            f32::INFINITY
        } else {
            (1.0 / dir[1]).abs()
        },
        if dir[2] == 0.0 {
            f32::INFINITY
        } else {
            (1.0 / dir[2]).abs()
        },
    ];
    let mut face;
    let mut t = 0.0f32;

    while t <= max_dist {
        let axis = if t_max[0] < t_max[1] && t_max[0] < t_max[2] {
            0
        } else if t_max[1] < t_max[2] {
            1
        } else {
            2
        };
        match axis {
            0 => {
                x += step_x;
                t = t_max[0];
                t_max[0] += t_delta[0];
                face = [-step_x, 0, 0];
            }
            1 => {
                y += step_y;
                t = t_max[1];
                t_max[1] += t_delta[1];
                face = [0, -step_y, 0];
            }
            _ => {
                z += step_z;
                t = t_max[2];
                t_max[2] += t_delta[2];
                face = [0, 0, -step_z];
            }
        }
        if t > max_dist {
            break;
        }
        let b = world.get_block(x, y, z);
        if def(b).solid {
            return Some(RayHit {
                x,
                y,
                z,
                block: b,
                face,
                dist: t,
            });
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocks::STONE;
    use crate::generation::{CHUNK_AXIS, CHUNK_HEIGHT};

    fn floor_world() -> World {
        let mut w = World::new(3);
        w.ensure_data(0, 0);
        for y in 0..CHUNK_HEIGHT {
            for z in 0..CHUNK_AXIS {
                for x in 0..CHUNK_AXIS {
                    let _ = w.set_block(x, y, z, crate::blocks::AIR);
                }
            }
        }
        for z in 0..CHUNK_AXIS {
            for x in 0..CHUNK_AXIS {
                let _ = w.set_block(x, 10, z, STONE);
            }
        }
        w
    }

    #[test]
    fn straight_down_hits_top_face() {
        let w = floor_world();
        let hit = raycast(&w, [8.5, 20.0, 8.5], [0.0, -1.0, 0.0], 15.0).expect("hit");
        assert_eq!((hit.x, hit.y, hit.z), (8, 10, 8));
        assert_eq!(hit.face, [0, 1, 0]);
        assert_eq!(hit.block, STONE);
    }

    #[test]
    fn short_ray_misses() {
        let w = floor_world();
        assert!(raycast(&w, [8.5, 20.0, 8.5], [0.0, -1.0, 0.0], 5.0).is_none());
    }

    #[test]
    fn diagonal_ray_reports_crossed_face() {
        let w = floor_world();
        let hit = raycast(&w, [8.5, 14.0, 8.5], [-0.5, -1.0, 0.0], 15.0).expect("hit");
        assert!(hit.y == 10 || hit.face[0] != 0 || hit.face[2] != 0);
        assert_eq!(hit.face[1], 1);
    }

    #[test]
    fn ray_along_positive_z_hits_wall_side_face() {
        let mut w = floor_world();
        for y in 11..14 {
            for x in 0..CHUNK_AXIS {
                let _ = w.set_block(x, y, 12, STONE);
            }
        }
        let hit = raycast(&w, [8.5, 12.5, 8.5], [0.0, 0.0, 1.0], 10.0).expect("hit");
        assert_eq!((hit.x, hit.y, hit.z), (8, 12, 12));
        assert_eq!(hit.face, [0, 0, -1]);
        assert_eq!(hit.block, STONE);
        assert!((hit.dist - 3.5).abs() < 1e-5, "dist={}", hit.dist);
    }

    #[test]
    fn water_is_not_targeted() {
        let mut w = floor_world();
        for y in 11..14 {
            let _ = w.set_block(4, y, 4, crate::blocks::WATER);
        }
        let hit = raycast(&w, [4.5, 20.0, 4.5], [0.0, -1.0, 0.0], 20.0).expect("hit");
        assert_eq!((hit.x, hit.y, hit.z), (4, 10, 4));
        assert_eq!(hit.block, STONE);
    }
}
