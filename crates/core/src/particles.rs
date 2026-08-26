//! Block debris: small textured squares thrown out of a broken or dug
//! block, pulled down by gravity and stopped by solid blocks. A pure
//! function of the seed and the calls made, like everything else here.

use crate::blocks::{BlockId, RenderKind, TileId, def};
use crate::rng::Lcg;
use crate::world::World;

/// One textured square in flight.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Particle {
    /// World-space center.
    pub pos: [f32; 3],
    /// Velocity in blocks per second.
    pub vel: [f32; 3],
    /// Seconds alive so far.
    pub age: f32,
    /// Seconds after which the particle disappears.
    pub life: f32,
    /// Atlas tile the square samples.
    pub tile: TileId,
    /// Tile-local origin of the sampled [`SUB_UV`] square.
    pub sub_uv: [f32; 2],
    /// Edge length in blocks.
    pub size: f32,
}

/// Fraction of a tile one particle shows on each axis.
pub const SUB_UV: f32 = 0.25;
/// Upper bound on live particles; the oldest make room past it.
pub const MAX: usize = 1024;
/// Particles one broken block throws out.
pub const BREAK_COUNT: usize = 24;

const GRAVITY: f32 = 20.0;
const DRAG_PER_S: f32 = 1.5;

/// Every live particle plus the generator that spawned them.
pub struct Particles {
    list: Vec<Particle>,
    rng: Lcg,
}

fn side_tile(block: BlockId) -> Option<TileId> {
    let d = def(block);
    (d.render != RenderKind::Air).then_some(d.tiles[0])
}

impl Particles {
    /// An empty pool seeded with `seed`.
    #[must_use]
    pub const fn new(seed: u32) -> Self {
        Self {
            list: Vec::new(),
            rng: Lcg::new(seed),
        }
    }

    /// Live particles, oldest first.
    #[must_use]
    pub fn as_slice(&self) -> &[Particle] {
        &self.list
    }

    /// Number of live particles.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.list.len()
    }

    /// Whether nothing is in flight.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    /// Drops the oldest particles so that `extra` more fit under [`MAX`].
    fn make_room(&mut self, extra: usize) {
        let over = (self.list.len() + extra).saturating_sub(MAX);
        if over > 0 {
            self.list.drain(..over);
        }
    }

    fn sub_uv(&mut self) -> [f32; 2] {
        let cells = (1.0 / SUB_UV) as u32;
        let pick = |rng: &mut Lcg| ((rng.next_f32() * cells as f32) as u32).min(cells - 1);
        let u = pick(&mut self.rng);
        let v = pick(&mut self.rng);
        [u as f32 * SUB_UV, v as f32 * SUB_UV]
    }

    /// Throws [`BREAK_COUNT`] fragments of `block` out of `cell`.
    pub fn spawn_break(&mut self, cell: [i32; 3], block: BlockId) {
        let Some(tile) = side_tile(block) else { return };
        self.make_room(BREAK_COUNT);
        for _ in 0..BREAK_COUNT {
            let offset = [
                self.rng.range(0.15, 0.85),
                self.rng.range(0.15, 0.85),
                self.rng.range(0.15, 0.85),
            ];
            let burst = self.rng.range(4.0, 6.0);
            let upward = self.rng.range(3.0, 5.0);
            let life = self.rng.range(0.5, 1.0);
            let size = self.rng.range(0.08, 0.14);
            let sub_uv = self.sub_uv();
            self.list.push(Particle {
                pos: [
                    cell[0] as f32 + offset[0],
                    cell[1] as f32 + offset[1],
                    cell[2] as f32 + offset[2],
                ],
                vel: [
                    (offset[0] - 0.5) * burst,
                    (offset[1] - 0.5) * burst + upward,
                    (offset[2] - 0.5) * burst,
                ],
                age: 0.0,
                life,
                tile,
                sub_uv,
                size,
            });
        }
    }

    /// Chips one or two fragments of `block` off the face of `cell` that
    /// faces `from`.
    pub fn spawn_dig(&mut self, cell: [i32; 3], block: BlockId, from: [f32; 3]) {
        let Some(tile) = side_tile(block) else { return };
        let delta = [
            from[0] - (cell[0] as f32 + 0.5),
            from[1] - (cell[1] as f32 + 0.5),
            from[2] - (cell[2] as f32 + 0.5),
        ];
        let axis = (1..3).fold(0, |best, i| {
            if delta[i].abs() >= delta[best].abs() {
                i
            } else {
                best
            }
        });
        let sign = if delta[axis] < 0.0 { -1.0 } else { 1.0 };
        let count = 1 + usize::from(self.rng.next_f32() < 0.5);
        self.make_room(count);
        for _ in 0..count {
            let mut offset = [
                self.rng.range(0.1, 0.9),
                self.rng.range(0.1, 0.9),
                self.rng.range(0.1, 0.9),
            ];
            offset[axis] = 0.5 + sign * 0.55;
            let mut vel = [
                self.rng.range(-1.0, 1.0),
                self.rng.range(-1.0, 1.0),
                self.rng.range(-1.0, 1.0),
            ];
            vel[axis] += sign * 1.5;
            vel[1] += 1.0;
            let life = self.rng.range(0.3, 0.6);
            let size = self.rng.range(0.06, 0.1);
            let sub_uv = self.sub_uv();
            self.list.push(Particle {
                pos: [
                    cell[0] as f32 + offset[0],
                    cell[1] as f32 + offset[1],
                    cell[2] as f32 + offset[2],
                ],
                vel,
                age: 0.0,
                life,
                tile,
                sub_uv,
                size,
            });
        }
    }

    /// Advances every particle by `dt` seconds: gravity, drag, per-axis
    /// collision against solid blocks, then expiry.
    pub fn step(&mut self, dt: f32, world: &World) {
        let drag = (1.0 - DRAG_PER_S * dt).max(0.0);
        for p in &mut self.list {
            p.age += dt;
            p.vel[1] -= GRAVITY * dt;
            for v in &mut p.vel {
                *v *= drag;
            }
            for axis in 0..3 {
                let next = p.pos[axis] + p.vel[axis] * dt;
                let mut probe = p.pos;
                probe[axis] = next;
                let blocked = world.is_solid_at(
                    probe[0].floor() as i32,
                    probe[1].floor() as i32,
                    probe[2].floor() as i32,
                );
                if blocked {
                    p.vel[axis] = 0.0;
                    if axis == 1 {
                        p.vel[0] *= 0.5;
                        p.vel[2] *= 0.5;
                    }
                } else {
                    p.pos[axis] = next;
                }
            }
        }
        self.list.retain(|p| p.age < p.life);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocks::{AIR, STONE};

    const DT: f32 = 1.0 / 120.0;

    fn air_world() -> World {
        let mut w = World::with_stages(1, vec![]);
        w.ensure_around(0, 0, 1);
        w
    }

    fn bits(p: &Particles) -> Vec<u32> {
        p.as_slice()
            .iter()
            .flat_map(|p| {
                p.pos
                    .into_iter()
                    .chain(p.vel)
                    .chain([p.age, p.life, p.size])
            })
            .map(f32::to_bits)
            .collect()
    }

    #[test]
    fn spawn_break_is_deterministic() {
        let mut a = Particles::new(7);
        let mut b = Particles::new(7);
        a.spawn_break([8, 30, 8], STONE);
        b.spawn_break([8, 30, 8], STONE);
        assert_eq!(a.len(), BREAK_COUNT);
        assert_eq!(bits(&a), bits(&b));
        assert!(a.as_slice().iter().all(|p| p.tile == def(STONE).tiles[0]));
        assert!(
            a.as_slice()
                .iter()
                .all(|p| p.sub_uv.iter().all(|c| *c <= 1.0 - SUB_UV))
        );
    }

    #[test]
    fn air_spawns_nothing() {
        let mut p = Particles::new(1);
        p.spawn_break([0, 0, 0], AIR);
        p.spawn_dig([0, 0, 0], AIR, [0.5, 5.0, 0.5]);
        assert!(p.is_empty());
    }

    #[test]
    fn dig_emits_one_or_two_on_the_facing_side() {
        let mut p = Particles::new(3);
        p.spawn_dig([8, 10, 8], STONE, [8.5, 20.0, 8.5]);
        assert!((1..=2).contains(&p.len()));
        assert!(p.as_slice().iter().all(|q| q.pos[1] > 11.0));
        let mut side = Particles::new(3);
        side.spawn_dig([8, 10, 8], STONE, [-5.0, 10.5, 8.5]);
        assert!(side.as_slice().iter().all(|q| q.pos[0] < 8.0));
    }

    #[test]
    fn particles_fall_and_expire() {
        let world = air_world();
        let mut p = Particles::new(5);
        p.spawn_break([8, 30, 8], STONE);
        let before: f32 = p.as_slice().iter().map(|q| q.vel[1]).sum();
        p.step(DT, &world);
        let after: f32 = p.as_slice().iter().map(|q| q.vel[1]).sum();
        assert!(after < before);
        for _ in 0..240 {
            p.step(DT, &world);
        }
        assert!(p.is_empty());
    }

    #[test]
    fn particles_rest_on_solid_ground() {
        let mut world = air_world();
        for x in 4..=12 {
            for z in 4..=12 {
                let _ = world.set_block(x, 10, z, STONE);
            }
        }
        let mut p = Particles::new(9);
        p.spawn_break([8, 11, 8], STONE);
        for _ in 0..54 {
            p.step(DT, &world);
        }
        assert!(!p.is_empty());
        assert!(p.as_slice().iter().all(|q| q.pos[1] >= 11.0));
        assert!(p.as_slice().iter().any(|q| q.vel[1].to_bits() == 0));
    }

    #[test]
    fn spawn_caps_at_max() {
        let mut p = Particles::new(2);
        for _ in 0..50 {
            p.spawn_break([0, 5, 0], STONE);
        }
        assert_eq!(p.len(), MAX);
    }

    #[test]
    fn zero_dt_changes_nothing() {
        let world = air_world();
        let mut p = Particles::new(11);
        p.spawn_break([8, 30, 8], STONE);
        let before = bits(&p);
        p.step(0.0, &world);
        assert_eq!(bits(&p), before);
    }
}
