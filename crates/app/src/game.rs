//! Game state, discrete actions, and the day-night sun cycle.

use ox_core::blocks::{AIR, BlockId, HOTBAR, break_seconds, material};
use ox_core::generation::CHUNK_AXIS;
use ox_core::generation::settings::WorldGenSettings;
use ox_core::generation::terrain::column_info;
use ox_core::player::{HALF, Input, PHEIGHT, Player};
use ox_core::raycast::{RayHit, raycast};
use ox_core::world::{HEIGHT, World};

/// Reach of the block-targeting view ray, in blocks.
pub const REACH: f32 = 5.0;

/// Chunk radius around the player within which [`Game::update`] guarantees
/// loaded data before physics runs (`docs/adr/0003`).
pub const LOAD_RADIUS: i32 = 2;

/// Length of one full day-night cycle in simulated seconds.
pub const DAY_LENGTH: f32 = 600.0;

/// Fraction of [`DAY_LENGTH`] already elapsed when a game starts.
pub const DAY_START_FRACTION: f32 = 0.3;

/// Seed used by [`Game::new`] when no explicit seed is given.
pub const DEFAULT_SEED: i32 = 1337;

/// Spawn column coordinate inside the pre-generated spawn area.
pub const SPAWN_COLUMN: i32 = 8;

/// Spawn x/z position: the center of [`SPAWN_COLUMN`].
pub const SPAWN_XZ: f32 = SPAWN_COLUMN as f32 + 0.5;

/// Gap between the spawn feet position and the ground surface.
pub const SPAWN_CLEARANCE: f32 = 0.01;

/// Fall below this y and the player respawns at the spawn point.
pub const VOID_Y: f32 = -16.0;

/// Seconds after a break before the next block starts taking damage.
pub const BREAK_COOLDOWN: f32 = 0.25;

/// Seconds between two dig hits on the block under attack.
pub const DIG_PERIOD: f32 = 0.25;

/// A discrete gameplay action produced by input handling.
pub enum Action {
    /// Place the selected block against the targeted face.
    Place,
    /// Select the hotbar slot holding the targeted block.
    Pick,
    /// Select a hotbar slot by index.
    Select(usize),
    /// Toggle flight mode.
    ToggleFly,
}

/// A presentation event the simulation reports and the shell drains for
/// arm swing, particles and sound; the digest ignores it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    /// A block left the world.
    Broke {
        /// Cell that held the block.
        cell: [i32; 3],
        /// Block that was removed.
        block: BlockId,
    },
    /// A block entered the world.
    Placed {
        /// Cell that now holds the block.
        cell: [i32; 3],
        /// Block that was placed.
        block: BlockId,
    },
    /// One dig hit landed on a block still standing.
    Dig {
        /// Cell under attack.
        cell: [i32; 3],
        /// Block under attack.
        block: BlockId,
    },
}

/// A block under sustained attack.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Breaking {
    /// Cell being broken.
    pub cell: [i32; 3],
    /// Fraction of the break time elapsed, in `0.0..1.0`.
    pub progress: f32,
}

/// Dig hits landed once `progress` of a break that takes `secs` elapsed;
/// the first lands the moment the attack starts.
fn dig_hits(progress: f32, secs: f32) -> u32 {
    (progress * secs / DIG_PERIOD) as u32 + 1
}

/// Full simulation state: voxel world, player, hotbar selection, day time,
/// the block under attack, pending mesh invalidations and pending
/// presentation effects.
pub struct Game {
    /// World the simulation runs against.
    pub world: World,
    /// Player body and movement state.
    pub player: Player,
    /// Hotbar index of the currently selected block.
    pub selected: usize,
    /// Time of day in seconds, wrapping at [`DAY_LENGTH`].
    pub time: f32,
    /// Block under sustained attack, if any.
    pub breaking: Option<Breaking>,
    break_cooldown: f32,
    seed: i32,
    spawn: [f32; 3],
    /// Chunk coordinates whose meshes are stale after block edits.
    pub pending_remesh: Vec<(i32, i32)>,
    /// Effects reported since the shell last drained them.
    pub pending_effects: Vec<Effect>,
}

impl Default for Game {
    fn default() -> Self {
        Self::new()
    }
}

impl Game {
    /// A game generated from [`DEFAULT_SEED`].
    pub fn new() -> Self {
        Self::with_seed(DEFAULT_SEED)
    }

    /// A game whose terrain derives deterministically from `seed`, with the
    /// spawn area loaded so queries work before the first tick.
    pub fn with_seed(seed: i32) -> Self {
        let mut world = World::new(seed);
        let h = column_info(
            SPAWN_COLUMN,
            SPAWN_COLUMN,
            seed,
            &WorldGenSettings::DEFAULTS,
        )
        .h;
        let spawn = [SPAWN_XZ, (h + 1) as f32 + SPAWN_CLEARANCE, SPAWN_XZ];
        let (scx, scz) = chunk_of(spawn);
        world.ensure_around(scx, scz, LOAD_RADIUS);
        Self {
            world,
            player: Player::new(spawn),
            selected: 0,
            time: DAY_LENGTH * DAY_START_FRACTION,
            breaking: None,
            break_cooldown: 0.0,
            seed,
            spawn,
            pending_remesh: Vec::new(),
            pending_effects: Vec::new(),
        }
    }

    /// The seed this game's terrain was generated from.
    pub const fn seed(&self) -> i32 {
        self.seed
    }

    /// Chunk coordinates containing the player's feet.
    #[must_use]
    pub fn player_chunk(&self) -> (i32, i32) {
        chunk_of(self.player.pos)
    }

    /// Advances the simulation by `dt` seconds using one input snapshot.
    ///
    /// Loads every chunk within [`LOAD_RADIUS`] of the player first, so
    /// physics never meets unloaded ground.
    pub fn update(&mut self, dt: f32, input: Input) {
        let (cx, cz) = self.player_chunk();
        self.world.ensure_around(cx, cz, LOAD_RADIUS);
        self.dig(dt, input.attack);
        self.player.update(&self.world, dt, &input);
        self.time = (self.time + dt) % DAY_LENGTH;
        if self.player.pos[1] < VOID_Y {
            self.player.pos = self.spawn;
            self.player.vel = [0.0; 3];
        }
    }

    fn dig(&mut self, dt: f32, attack: bool) {
        self.break_cooldown = (self.break_cooldown - dt).max(0.0);
        let target = if attack && self.break_cooldown <= 0.0 {
            self.target()
        } else {
            None
        };
        let Some((hit, secs)) =
            target.and_then(|h| break_seconds(material(h.block)).map(|s| (h, s)))
        else {
            self.breaking = None;
            return;
        };
        let cell = [hit.x, hit.y, hit.z];
        let resumed = self.breaking.filter(|b| b.cell == cell);
        let step = dt / secs;
        let progress = resumed.map_or(0.0, |b| b.progress) + step;
        if progress + 0.5 * step >= 1.0 {
            let affected = self.world.set_block(hit.x, hit.y, hit.z, AIR);
            self.pending_remesh.extend(affected);
            self.pending_effects.push(Effect::Broke {
                cell,
                block: hit.block,
            });
            self.breaking = None;
            self.break_cooldown = BREAK_COOLDOWN;
            return;
        }
        let landed = resumed.map_or(0, |b| dig_hits(b.progress, secs));
        for _ in landed..dig_hits(progress, secs) {
            self.pending_effects.push(Effect::Dig {
                cell,
                block: hit.block,
            });
        }
        self.breaking = Some(Breaking { cell, progress });
    }

    /// Moves the player to `pos` at rest; the next tick loads the ground there.
    pub const fn teleport(&mut self, pos: [f32; 3]) {
        self.player.pos = pos;
        self.player.vel = [0.0; 3];
        self.player.on_ground = false;
    }

    fn look_dir(&self) -> [f32; 3] {
        let (sy, cy) = self.player.yaw.sin_cos();
        let (sp, cp) = self.player.pitch.sin_cos();
        [-sy * cp, sp, -cy * cp]
    }

    /// First cell hit by the view ray within reach, if any.
    pub fn target(&self) -> Option<RayHit> {
        raycast(
            &self.world,
            self.player.eye_position(),
            self.look_dir(),
            REACH,
        )
    }

    /// Applies one action to the world.
    pub fn handle(&mut self, action: &Action) {
        match action {
            Action::ToggleFly => self.player.flying = !self.player.flying,
            Action::Select(i) => self.selected = (*i).min(HOTBAR.len() - 1),
            Action::Pick => {
                if let Some(hit) = self.target()
                    && let Some(i) = HOTBAR.iter().position(|b| *b == hit.block)
                {
                    self.selected = i;
                }
            }
            Action::Place => {
                if let Some(hit) = self.target() {
                    let tx = hit.x + hit.face[0];
                    let ty = hit.y + hit.face[1];
                    let tz = hit.z + hit.face[2];
                    if (0..HEIGHT).contains(&ty)
                        && self.world.get_block(tx, ty, tz) == AIR
                        && !overlaps_player(&self.player.pos, tx, ty, tz)
                    {
                        let block = HOTBAR[self.selected];
                        let affected = self.world.set_block(tx, ty, tz, block);
                        self.pending_remesh.extend(affected);
                        self.pending_effects.push(Effect::Placed {
                            cell: [tx, ty, tz],
                            block,
                        });
                    }
                }
            }
        }
    }
}

fn chunk_of(pos: [f32; 3]) -> (i32, i32) {
    (
        pos[0].div_euclid(CHUNK_AXIS as f32) as i32,
        pos[2].div_euclid(CHUNK_AXIS as f32) as i32,
    )
}

fn overlaps_player(pos: &[f32; 3], bx: i32, by: i32, bz: i32) -> bool {
    let [px, py, pz] = *pos;
    (bx as f32) < px + HALF
        && (bx as f32) + 1.0 > px - HALF
        && (by as f32) < py + PHEIGHT
        && (by as f32) + 1.0 > py
        && (bz as f32) < pz + HALF
        && (bz as f32) + 1.0 > pz - HALF
}

/// Sky lighting parameters for one moment of the day-night cycle.
pub struct SunState {
    /// Normalized direction toward the sun.
    pub dir: [f32; 3],
    /// Sun light color.
    pub color: [f32; 3],
    /// Sky gradient color at the zenith.
    pub zenith: [f32; 3],
    /// Sky gradient color at the horizon.
    pub horizon: [f32; 3],
}

/// Computes the [`SunState`] for a time-of-day value in `[0, DAY_LENGTH)`.
pub fn sun_state(time: f32) -> SunState {
    let ang = time / DAY_LENGTH * std::f32::consts::TAU;
    let (sa, ca) = ang.sin_cos();
    let elev = sa;
    let len = (ca * ca + sa * sa + 0.35 * 0.35).sqrt();
    let dir = [ca / len, sa / len, 0.35 / len];
    let day = ((elev + 0.15) / 0.35).clamp(0.0, 1.0);
    let warmth = 1.0 - (elev.abs() / 0.3).min(1.0);
    let mix = |a: [f32; 3], b: [f32; 3], t: f32| {
        [
            a[0] + (b[0] - a[0]) * t,
            a[1] + (b[1] - a[1]) * t,
            a[2] + (b[2] - a[2]) * t,
        ]
    };
    let zenith = mix([0.012, 0.02, 0.06], [0.30, 0.54, 0.86], day);
    let mut horizon = mix([0.04, 0.05, 0.10], [0.70, 0.82, 0.94], day);
    horizon = mix(horizon, [0.98, 0.55, 0.32], warmth * 0.55);
    let mut color = mix([0.10, 0.12, 0.20], [1.0, 0.98, 0.93], day);
    color = mix(color, [1.0, 0.62, 0.38], warmth * 0.5);
    SunState {
        dir,
        color,
        zenith,
        horizon,
    }
}
