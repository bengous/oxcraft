//! Deterministic replay harness driving the game with synthetic input.

use serde_json::json;

use ox_core::blocks::BlockId;
use ox_core::digest::Fnv1a;

use crate::game::Game;
use crate::input::{InputState, synthetic_key};
use crate::test_server::Command;

/// Fixed simulation timestep: 1/120 s.
pub const FIXED_DT: f32 = 1.0 / 120.0;

/// Seed shared by replay tests and external drivers for reproducible worlds.
pub const TEST_SEED: i32 = 424_242;

/// Borrowing facade that drives one input state and one [`Game`] exactly
/// like the binary's event loop does.
pub struct GameHarness<'a> {
    /// Input state feeding the simulation.
    pub input: &'a mut InputState,
    /// Simulated game.
    pub game: &'a mut Game,
}

/// Snapshot of player position, velocity, and mode flags.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayerState {
    /// World-space position.
    pub pos: [f32; 3],
    /// Velocity.
    pub vel: [f32; 3],
    /// Whether the player stands on ground this tick.
    pub on_ground: bool,
    /// Whether flight mode is active.
    pub flying: bool,
    /// Hotbar index of the selected block.
    pub selected: usize,
    /// Time of day.
    pub time: f32,
}

/// What [`GameHarness::apply`] did with one [`Command`].
pub enum Applied {
    /// Harness-handled verb; carries the bare JSON result object.
    Reply(serde_json::Value),
    /// Simulating verb (`tick`, `run`): the shell advances presentation by
    /// the same steps and streams around the player before replying.
    Ticked {
        /// Fixed steps the simulation advanced.
        ticks: usize,
        /// Bare JSON result object.
        result: serde_json::Value,
    },
    /// Shell-owned verb (`screenshot`, `quit`): the caller renders or exits.
    Shell,
}

impl<'a> GameHarness<'a> {
    /// Borrows an input state and a game into a harness.
    pub const fn wrap(input: &'a mut InputState, game: &'a mut Game) -> Self {
        Self { input, game }
    }

    /// Marks the view as grabbing input, enabling key handling.
    pub const fn grab(&mut self) {
        self.input.grabbed = true;
    }

    /// Presses a named key and keeps it held until [`Self::release`].
    pub fn hold(&mut self, key: &str) {
        if let Some(ev) = synthetic_key(key, true) {
            self.input.key(&ev);
        }
    }

    /// Releases a held named key.
    pub fn release(&mut self, key: &str) {
        if let Some(ev) = synthetic_key(key, false) {
            self.input.key(&ev);
        }
    }

    /// Holds then immediately releases a named key.
    pub fn press(&mut self, key: &str) {
        self.hold(key);
        self.release(key);
    }

    /// Presses a named mouse button; `"left"` holds the attack until
    /// [`Self::mouse_release`].
    pub fn mouse_press(&mut self, button: &str) {
        self.input.mouse(mouse_button(button), true);
    }

    /// Releases a named mouse button.
    pub fn mouse_release(&mut self, button: &str) {
        self.input.mouse(mouse_button(button), false);
    }

    /// Sets the player's yaw and pitch directly.
    pub const fn look(&mut self, yaw: f32, pitch: f32) {
        self.game.player.yaw = yaw;
        self.game.player.pitch = pitch;
    }

    /// Moves the player to `pos` at rest.
    pub const fn teleport(&mut self, pos: [f32; 3]) {
        self.game.teleport(pos);
    }

    /// Applies all queued input actions to the game.
    pub fn process_actions(&mut self) {
        for action in std::mem::take(&mut self.input.actions) {
            self.game.handle(&action);
        }
    }

    /// Processes actions, then advances `n` fixed timesteps.
    pub fn tick(&mut self, n: usize) {
        self.process_actions();
        for _ in 0..n {
            self.game.update(FIXED_DT, self.input.input);
        }
    }

    /// Advances the simulation by roughly `seconds` of fixed-timestep ticks
    /// and reports how many it took.
    pub fn run(&mut self, seconds: f32) -> usize {
        let n = (seconds / FIXED_DT).ceil() as usize;
        self.tick(n);
        n
    }

    /// Snapshot of player position, velocity, and mode flags.
    pub const fn player(&self) -> PlayerState {
        let p = &self.game.player;
        PlayerState {
            pos: p.pos,
            vel: p.vel,
            on_ground: p.on_ground,
            flying: p.flying,
            selected: self.game.selected,
            time: self.game.time,
        }
    }

    /// Reads one world cell.
    pub fn block_at(&self, x: i32, y: i32, z: i32) -> BlockId {
        self.game.world.get_block(x, y, z)
    }

    /// FNV-1a over the player state, then each listed cell's block id. The
    /// golden both transports compare; cells arrive in the caller's order.
    #[must_use]
    pub fn digest(&self, cells: &[[i32; 3]]) -> u64 {
        let p = self.player();
        let mut digest = Fnv1a::new();
        for value in p.pos.into_iter().chain(p.vel).chain([p.time]) {
            digest.write_f32(value);
        }
        digest.write(&[u8::from(p.on_ground), u8::from(p.flying)]);
        digest.write(&(p.selected as u64).to_le_bytes());
        for cell in cells {
            digest.write(&[self.block_at(cell[0], cell[1], cell[2])]);
        }
        digest.finish()
    }

    /// Targeted cell as `(x, y, z, block, face-normal)` if the view ray hits.
    pub fn target_cell(&self) -> Option<(i32, i32, i32, BlockId, [i32; 3])> {
        self.game.target().map(|h| (h.x, h.y, h.z, h.block, h.face))
    }

    /// Display name of the selected hotbar block.
    pub fn selected_name(&self) -> &'static str {
        ox_core::blocks::def(ox_core::blocks::HOTBAR[self.game.selected]).name
    }

    /// Executes one transport command on input and game. The single dispatch
    /// shared by replay tests and the socket server; screenshot and quit stay
    /// with the shell ([`Applied::Shell`]).
    pub fn apply(&mut self, command: &Command) -> Applied {
        match command {
            Command::Press { key } => {
                self.grab();
                self.press(key);
                Applied::Reply(json!({ "key": key }))
            }
            Command::Hold { key } => {
                self.grab();
                self.hold(key);
                Applied::Reply(json!({ "key": key }))
            }
            Command::Release { key } => {
                self.release(key);
                Applied::Reply(json!({ "released": key }))
            }
            Command::Mouse { button } => {
                self.mouse_press(button);
                Applied::Reply(json!({ "clicked": button }))
            }
            Command::MouseRelease { button } => {
                self.mouse_release(button);
                Applied::Reply(json!({ "released": button }))
            }
            Command::Look { yaw, pitch } => {
                self.look(*yaw, *pitch);
                Applied::Reply(json!({ "yaw": yaw, "pitch": pitch }))
            }
            Command::Teleport { x, y, z } => {
                self.teleport([*x, *y, *z]);
                Applied::Reply(json!({ "pos": [x, y, z] }))
            }
            Command::Tick { n } => {
                self.tick(*n);
                Applied::Ticked {
                    ticks: *n,
                    result: json!({ "ticked": n, "pos": self.game.player.pos }),
                }
            }
            Command::Run { seconds } => {
                let ticks = self.run(*seconds);
                Applied::Ticked {
                    ticks,
                    result: json!({ "ran_seconds": seconds, "pos": self.game.player.pos }),
                }
            }
            Command::Target => Applied::Reply(json!({ "target": self.target_cell() })),
            Command::State => {
                let p = self.player();
                Applied::Reply(json!({
                    "pos": p.pos,
                    "vel": p.vel,
                    "on_ground": p.on_ground,
                    "flying": p.flying,
                    "selected": p.selected,
                    "selected_name": self.selected_name(),
                    "time": p.time,
                    "breaking": self.game.breaking.map(|b| json!({
                        "cell": b.cell,
                        "progress": b.progress,
                    })),
                }))
            }
            Command::Block { x, y, z } => Applied::Reply(json!({
                "x": x,
                "y": y,
                "z": z,
                "block": self.block_at(*x, *y, *z),
            })),
            Command::Digest { cells } => Applied::Reply(json!({ "digest": self.digest(cells) })),
            Command::Screenshot { .. } | Command::Quit => Applied::Shell,
        }
    }
}

fn mouse_button(name: &str) -> winit::event::MouseButton {
    match name {
        "right" => winit::event::MouseButton::Right,
        "middle" => winit::event::MouseButton::Middle,
        _ => winit::event::MouseButton::Left,
    }
}
