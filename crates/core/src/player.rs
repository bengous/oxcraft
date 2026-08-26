//! Player physics: AABB collision, walking/flying movement, water handling.

use crate::blocks::{RenderKind, def};
use crate::world::World;

/// Gravity acceleration in blocks/s².
pub const G: f32 = 27.0;
/// Vertical jump impulse in blocks/s.
pub const JUMP: f32 = 8.6;
/// Walking speed in blocks/s.
pub const WALK: f32 = 4.3;
/// Sprinting speed on ground in blocks/s.
pub const SPRINT: f32 = 6.8;
/// Flying speed in blocks/s.
pub const FLY: f32 = 11.0;
/// Sprinting flight speed in blocks/s.
pub const FLY_SPRINT: f32 = 18.0;
/// Vertical fly speed in blocks/s.
pub const FLY_VERT: f32 = 9.0;
/// Half width of the player AABB in blocks.
pub const HALF: f32 = 0.3;
/// Full height of the player AABB in blocks.
pub const PHEIGHT: f32 = 1.8;
/// Eye height above the player origin in blocks.
pub const EYE: f32 = 1.62;

const EPS: f32 = 1e-3;
const STEP_DT: f32 = 0.008;

/// Movement key snapshot consumed by [`Player::update`].
#[derive(Clone, Copy, Default)]
pub struct Input {
    /// Move forward (relative to yaw).
    pub forward: bool,
    /// Move backward.
    pub back: bool,
    /// Strafe left.
    pub left: bool,
    /// Strafe right.
    pub right: bool,
    /// Jump (or ascend while flying).
    pub jump: bool,
    /// Descend while flying.
    pub down: bool,
    /// Sprint modifier.
    pub sprint: bool,
    /// Attack held (left mouse); consumed by the game, ignored by physics.
    pub attack: bool,
}

/// Positional/kinematic player state moved through the world each tick.
pub struct Player {
    /// Feet-center position in world space.
    pub pos: [f32; 3],
    /// Velocity in blocks/s.
    pub vel: [f32; 3],
    /// Horizontal look angle in radians.
    pub yaw: f32,
    /// Vertical look angle in radians.
    pub pitch: f32,
    /// Whether the player stands on ground this tick.
    pub on_ground: bool,
    /// Creative-style flight mode toggle.
    pub flying: bool,
}

impl Player {
    /// A stationary player at `pos` looking down -Z.
    pub const fn new(pos: [f32; 3]) -> Self {
        Self {
            pos,
            vel: [0.0; 3],
            yaw: 0.0,
            pitch: 0.0,
            on_ground: false,
            flying: false,
        }
    }

    /// Camera position for view-matrix construction.
    #[must_use]
    pub const fn eye_position(&self) -> [f32; 3] {
        [self.pos[0], self.pos[1] + EYE, self.pos[2]]
    }

    fn overlaps_block(&self, x: i32, y: i32, z: i32) -> bool {
        let [px, py, pz] = self.pos;
        (x as f32) < px + HALF
            && (x as f32) + 1.0 > px - HALF
            && (y as f32) < py + PHEIGHT
            && (y as f32) + 1.0 > py
            && (z as f32) < pz + HALF
            && (z as f32) + 1.0 > pz - HALF
    }

    fn is_in_water(&self, world: &World) -> bool {
        let feet = world.get_block(
            self.pos[0].floor() as i32,
            (self.pos[1] + 0.4).floor() as i32,
            self.pos[2].floor() as i32,
        );
        let eye = world.get_block(
            self.pos[0].floor() as i32,
            (self.pos[1] + EYE).floor() as i32,
            self.pos[2].floor() as i32,
        );
        def(feet).render == RenderKind::Liquid || def(eye).render == RenderKind::Liquid
    }

    fn collide_axis(&mut self, world: &World, axis: usize, delta: f32) {
        if delta == 0.0 {
            return;
        }
        self.pos[axis] += delta;
        let min_x = (self.pos[0] - HALF).floor() as i32;
        let max_x = (self.pos[0] + HALF).floor() as i32;
        let min_y = self.pos[1].floor() as i32;
        let max_y = (self.pos[1] + PHEIGHT).floor() as i32;
        let min_z = (self.pos[2] - HALF).floor() as i32;
        let max_z = (self.pos[2] + HALF).floor() as i32;
        let mut bound: Option<i32> = None;
        for y in min_y..=max_y {
            for z in min_z..=max_z {
                for x in min_x..=max_x {
                    if !world.is_solid_at(x, y, z) {
                        continue;
                    }
                    if !self.overlaps_block(x, y, z) {
                        continue;
                    }
                    let c = match axis {
                        0 => x,
                        1 => y,
                        _ => z,
                    };
                    bound =
                        Some(bound.map_or(c, |b| if delta > 0.0 { b.min(c) } else { b.max(c) }));
                }
            }
        }
        let Some(bound) = bound else { return };
        match axis {
            0 => {
                self.pos[0] = if delta > 0.0 {
                    bound as f32 - HALF - EPS
                } else {
                    bound as f32 + 1.0 + HALF + EPS
                };
                self.vel[0] = 0.0;
            }
            1 => {
                if delta > 0.0 {
                    self.pos[1] = bound as f32 - PHEIGHT - EPS;
                } else {
                    self.pos[1] = bound as f32 + 1.0 + EPS;
                    self.on_ground = true;
                }
                self.vel[1] = 0.0;
            }
            _ => {
                self.pos[2] = if delta > 0.0 {
                    bound as f32 - HALF - EPS
                } else {
                    bound as f32 + 1.0 + HALF + EPS
                };
                self.vel[2] = 0.0;
            }
        }
    }

    fn probe_ground(&self, world: &World) -> bool {
        if self.vel[1] > 0.0 {
            return false;
        }
        let layer = (self.pos[1] - 2.0 * EPS).floor() as i32;
        if layer < 0 {
            return false;
        }
        let min_x = (self.pos[0] - HALF).floor() as i32;
        let max_x = (self.pos[0] + HALF).floor() as i32;
        let min_z = (self.pos[2] - HALF).floor() as i32;
        let max_z = (self.pos[2] + HALF).floor() as i32;
        for z in min_z..=max_z {
            for x in min_x..=max_x {
                if world.is_solid_at(x, layer, z) {
                    return true;
                }
            }
        }
        false
    }

    /// Advances the simulation by `dt` seconds under `input`.
    ///
    /// Movement is integrated in fixed sub-steps of a few milliseconds so
    /// collisions stay stable regardless of frame pacing.
    pub fn update(&mut self, world: &World, dt: f32, input: &Input) {
        let steps = ((dt / STEP_DT).ceil() as usize).max(1);
        let h = dt / steps as f32;
        let in_water = self.is_in_water(world);
        let (sy, cy) = self.yaw.sin_cos();
        let fx = (i32::from(input.forward) - i32::from(input.back)) as f32;
        let rx = (i32::from(input.right) - i32::from(input.left)) as f32;
        let mut mx = fx * -sy + rx * cy;
        let mut mz = fx * -cy + rx * -sy;
        let ml = (mx * mx + mz * mz).sqrt();
        if ml > 0.0 {
            mx /= ml;
            mz /= ml;
        }
        for _ in 0..steps {
            if self.flying {
                let sp = if input.sprint { FLY_SPRINT } else { FLY };
                self.vel = [
                    mx * sp,
                    (i32::from(input.jump) - i32::from(input.down)) as f32 * FLY_VERT,
                    mz * sp,
                ];
            } else {
                let mut sp = if input.sprint { SPRINT } else { WALK };
                if in_water {
                    sp *= 0.5;
                }
                self.vel[0] = mx * sp;
                self.vel[2] = mz * sp;
                let g = if in_water { G * 0.3 } else { G };
                self.vel[1] -= g * h;
                let terminal = if in_water { -3.0 } else { -50.0 };
                if self.vel[1] < terminal {
                    self.vel[1] = terminal;
                }
                if input.jump {
                    if in_water {
                        self.vel[1] = 3.2;
                    } else if self.on_ground {
                        self.vel[1] = JUMP;
                    }
                }
            }
            self.on_ground = false;
            self.collide_axis(world, 1, self.vel[1] * h);
            if !self.on_ground {
                self.on_ground = self.probe_ground(world);
                if self.on_ground {
                    self.vel[1] = 0.0;
                }
            }
            self.collide_axis(world, 0, self.vel[0] * h);
            self.collide_axis(world, 2, self.vel[2] * h);
        }
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::float_cmp,
        reason = "velocity zeroing is exact by construction; exact equality pins physics regressions"
    )]
    use super::*;
    use crate::blocks::{STONE, WATER};
    use crate::generation::{CHUNK_AXIS, CHUNK_HEIGHT};

    fn flat_world() -> World {
        let mut w = World::new(1);
        w.ensure_data(0, 0);
        clear_chunk(&mut w);
        for z in 0..CHUNK_AXIS {
            for x in 0..CHUNK_AXIS {
                let _ = w.set_block(x, 10, z, STONE);
            }
        }
        w
    }

    fn clear_chunk(w: &mut World) {
        for y in 0..CHUNK_HEIGHT {
            for z in 0..CHUNK_AXIS {
                for x in 0..CHUNK_AXIS {
                    let _ = w.set_block(x, y, z, crate::blocks::AIR);
                }
            }
        }
    }

    fn water_world() -> World {
        let mut w = flat_world();
        for y in 11..=30 {
            for z in 0..CHUNK_AXIS {
                for x in 0..CHUNK_AXIS {
                    let _ = w.set_block(x, y, z, WATER);
                }
            }
        }
        w
    }

    fn settle(player: &mut Player, world: &World, seconds: f32) {
        let input = Input::default();
        let steps = (seconds / 0.05).ceil() as usize;
        let dt = seconds / steps as f32;
        for _ in 0..steps {
            player.update(world, dt, &input);
        }
    }

    #[test]
    fn player_lands_and_rests_on_floor() {
        let world = flat_world();
        let mut p = Player::new([8.5, 14.0, 8.5]);
        settle(&mut p, &world, 2.0);
        assert!(p.on_ground);
        assert!(p.pos[1] >= 11.0 && p.pos[1] < 11.01, "y={}", p.pos[1]);
        assert_eq!(p.vel[1], 0.0);
    }

    #[test]
    fn wall_stops_horizontal_movement() {
        let mut world = flat_world();
        for y in 11..15 {
            for z in 0..CHUNK_AXIS {
                let _ = world.set_block(12, y, z, STONE);
            }
        }
        let mut p = Player::new([10.5, 11.001, 8.5]);
        p.yaw = -std::f32::consts::FRAC_PI_2;
        let input = Input {
            forward: true,
            ..Default::default()
        };
        for _ in 0..60 {
            p.update(&world, 0.016, &input);
        }
        assert!(p.pos[0] < 11.71, "x={}", p.pos[0]);
        assert!(p.pos[0] > 11.5, "x={}", p.pos[0]);
    }

    #[test]
    fn jump_leaves_ground_and_returns() {
        let world = flat_world();
        let mut p = Player::new([8.5, 11.001, 8.5]);
        settle(&mut p, &world, 0.5);
        assert!(p.on_ground);
        let input = Input {
            jump: true,
            ..Default::default()
        };
        p.update(&world, 0.016, &input);
        assert!(p.vel[1] > 0.0);
        let mut airborne = false;
        for _ in 0..120 {
            p.update(&world, 0.016, &input);
            if p.pos[1] > 11.5 {
                airborne = true;
            }
        }
        assert!(airborne);
        settle(&mut p, &world, 1.0);
        assert!(p.on_ground);
    }

    #[test]
    fn walls_stop_movement_along_z() {
        let mut world = flat_world();
        for y in 11..15 {
            for x in 0..CHUNK_AXIS {
                let _ = world.set_block(x, y, 5, STONE);
                let _ = world.set_block(x, y, 12, STONE);
            }
        }
        let mut p = Player::new([8.5, 11.001, 8.5]);
        let forward = Input {
            forward: true,
            ..Default::default()
        };
        for _ in 0..120 {
            p.update(&world, 0.016, &forward);
        }
        assert!(p.pos[2] > 6.29 && p.pos[2] < 6.5, "z={}", p.pos[2]);
        assert_eq!(p.vel[2], 0.0);

        let back = Input {
            back: true,
            ..Default::default()
        };
        for _ in 0..120 {
            p.update(&world, 0.016, &back);
        }
        assert!(p.pos[2] > 11.5 && p.pos[2] < 11.71, "z={}", p.pos[2]);
        assert_eq!(p.vel[2], 0.0);
    }

    #[test]
    fn flying_holds_altitude_and_climbs_on_jump() {
        let world = flat_world();
        let mut p = Player::new([8.5, 20.0, 8.5]);
        p.flying = true;
        let idle = Input::default();
        for _ in 0..60 {
            p.update(&world, 0.016, &idle);
        }
        assert_eq!(p.pos[1], 20.0, "flight holds altitude without input");

        let up = Input {
            jump: true,
            ..Default::default()
        };
        p.update(&world, 0.5, &up);
        assert!(p.pos[1] > 24.0 && p.pos[1] < 25.0, "y={}", p.pos[1]);

        let down = Input {
            down: true,
            ..Default::default()
        };
        p.update(&world, 0.5, &down);
        assert!(p.pos[1] > 19.5 && p.pos[1] < 20.5, "y={}", p.pos[1]);
    }

    #[test]
    fn water_caps_fall_speed_and_lets_player_swim() {
        let world = water_world();
        let mut p = Player::new([8.5, 25.0, 8.5]);
        let idle = Input::default();
        for _ in 0..60 {
            p.update(&world, 0.016, &idle);
        }
        assert_eq!(p.vel[1], -3.0, "water terminal fall speed");

        let swim = Input {
            jump: true,
            forward: true,
            ..Default::default()
        };
        let y_before = p.pos[1];
        for _ in 0..30 {
            p.update(&world, 0.016, &swim);
        }
        assert!(p.pos[1] > y_before, "swimming rises: y={}", p.pos[1]);
        assert_eq!(p.vel[2], -WALK * 0.5, "water halves walking speed");
    }
}
