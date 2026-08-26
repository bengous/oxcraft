//! One running game bound to a renderer: the part shared by the interactive
//! shell and the headless modes.

use std::collections::HashSet;
use std::time::{Duration, Instant};

use ox_app::game::{DAY_LENGTH, Effect, Game, sun_state};
use ox_app::harness::{FIXED_DT, GameHarness};
use ox_app::input::InputState;
use ox_core::blocks::{HOTBAR, def, material};
use ox_core::mesher::build_mesh;
use ox_core::particles::Particles;
use ox_core::player::Input;
use ox_render::{
    CrackParams, FrameParams, HandParams, Overlay, Renderer, UiState, perspective_view,
};

use crate::audio::{Audio, SoundKind};
use crate::screens::{Screen, Screens};
use crate::stream::Streamer;
use crate::view::VIEW;

const VERSION: &str = concat!("oxcraft ", env!("CARGO_PKG_VERSION"));

/// Peak yaw offset of the idle menu camera, in radians.
const MENU_DRIFT_AMPLITUDE: f32 = 0.10;
/// Full menu camera sways per day. A whole number keeps the sway continuous
/// where [`DAY_LENGTH`] wraps back to zero.
const MENU_DRIFT_CYCLES_PER_DAY: u32 = 25;
/// Duration of one full menu camera sway, in simulated seconds.
const MENU_DRIFT_PERIOD_S: f32 = DAY_LENGTH / MENU_DRIFT_CYCLES_PER_DAY as f32;
/// Simulated seconds the sway takes to fade in when a menu opens, or out
/// when the player enters the world.
const MENU_BLEND_S: f32 = 0.35;

/// Presentation-only yaw offset of the menu camera; a pure function of the
/// simulated day time so repeated headless captures stay byte-identical.
fn menu_drift(time: f32) -> f32 {
    MENU_DRIFT_AMPLITUDE * (std::f32::consts::TAU * time / MENU_DRIFT_PERIOD_S).sin()
}

/// One easing step of the sway weight toward `target`, without overshoot.
fn blend_toward(current: f32, target: f32, dt: f32) -> f32 {
    let step = dt / MENU_BLEND_S;
    if target > current {
        (current + step).min(target)
    } else {
        (current - step).max(target)
    }
}

/// Ticks one arm swing lasts: 0.3 s at the fixed timestep.
const SWING_TICKS: u32 = 36;

/// Tick-driven arm swing timer.
#[derive(Default)]
struct Swing {
    remaining: u32,
}

impl Swing {
    const fn start(&mut self) {
        self.remaining = SWING_TICKS;
    }

    const fn start_if_idle(&mut self) {
        if self.remaining == 0 {
            self.start();
        }
    }

    const fn step(&mut self, ticks: u32) {
        self.remaining = self.remaining.saturating_sub(ticks);
    }

    fn phase(&self) -> f32 {
        if self.remaining == 0 {
            0.0
        } else {
            1.0 - self.remaining as f32 / SWING_TICKS as f32
        }
    }
}

pub(crate) struct Session {
    pub(crate) renderer: Renderer,
    pub(crate) game: Game,
    pub(crate) input: InputState,
    /// Menus open over the world; empty while the player has focus.
    screens: Screens,
    /// Cursor position in window pixels, while a menu is open and the
    /// window system has reported one. Only the shell tracks a pointer.
    pub(crate) cursor: Option<[f32; 2]>,
    /// Weight of the menu camera sway, 0 in the world and 1 under a menu.
    /// `None` until the first tick resolves it; an unticked one-shot capture
    /// reads the snapped target instead, never a partial fade.
    menu_blend: Option<f32>,
    streamer: Streamer,
    meshed: HashSet<(i32, i32)>,
    item_name_until: Option<Instant>,
    prev_selected: usize,
    swing: Swing,
    pub(crate) particles: Particles,
    /// Output stream for the block sounds; `None` keeps the game silent.
    pub(crate) audio: Option<Audio>,
}

impl Session {
    /// Starts a game at the default seed and uploads the spawn-area meshes.
    pub(crate) fn new(mut renderer: Renderer) -> Self {
        let mut game = Game::new();
        let streamer = Streamer::new();
        let mut meshed = HashSet::new();
        streamer.initial(&mut game, &mut renderer, &mut meshed);
        let prev_selected = game.selected;
        let particles = Particles::new(game.seed() as u32);
        Self {
            renderer,
            game,
            input: InputState::new(),
            screens: Screens::new(),
            cursor: None,
            menu_blend: None,
            streamer,
            meshed,
            item_name_until: None,
            prev_selected,
            swing: Swing::default(),
            particles,
            audio: None,
        }
    }

    pub(crate) const fn harness(&mut self) -> GameHarness<'_> {
        GameHarness::wrap(&mut self.input, &mut self.game)
    }

    /// Which full-screen overlay the current screen calls for, if any.
    pub(crate) fn overlay(&self) -> Option<Overlay> {
        self.screens.overlay()
    }

    /// Whether the world has focus with no menu over it.
    pub(crate) const fn playing(&self) -> bool {
        self.screens.playing()
    }

    /// Leaves every menu and gives the world focus.
    pub(crate) fn enter_world(&mut self) {
        self.screens.enter_world();
        self.sync_input();
    }

    /// Opens `screen` over the current one.
    pub(crate) fn open(&mut self, screen: Screen) {
        self.screens.open(screen);
        self.sync_input();
    }

    /// Scrolls the open list by `delta` pixels, kept inside `[0, max]`.
    pub(crate) fn scroll_by(&mut self, delta: f32, max: f32) {
        self.screens.scroll_by(delta, max);
    }

    /// Opens the pause menu, or leaves the top menu, per the current screen.
    pub(crate) fn escape(&mut self) {
        self.screens.escape();
        self.sync_input();
    }

    /// Mirrors the navigation state into the input gate. A menu never
    /// inherits held keys, and no screen inherits a stale cursor position.
    fn sync_input(&mut self) {
        let playing = self.screens.playing();
        self.input.grabbed = playing;
        self.cursor = None;
        if !playing {
            self.input.input = Input::default();
        }
    }

    /// Applies queued actions, advances `ticks` fixed steps, then the
    /// presentation those steps produced.
    pub(crate) fn advance(&mut self, ticks: usize) {
        self.harness().tick(ticks);
        self.present(ticks);
    }

    /// Advances everything the simulation does not own by `ticks` steps,
    /// then turns the effects it reported into an arm swing, particles and
    /// sound. Ageing before draining keeps a swing or a particle started
    /// this frame from losing the frame it was born in. The menu camera sway
    /// eases here too: it is presentation, not simulation.
    pub(crate) fn present(&mut self, ticks: usize) {
        self.swing.step(ticks as u32);
        for _ in 0..ticks {
            self.particles.step(FIXED_DT, &self.game.world);
        }
        self.drain_effects();
        if self.input.input.attack {
            self.swing.start_if_idle();
        }
        let target = f32::from(self.overlay().is_some());
        let blend = self.menu_blend.unwrap_or(target);
        self.menu_blend = Some(blend_toward(blend, target, ticks as f32 * FIXED_DT));
        self.refresh_item_name();
    }

    /// Consumes the effects the simulation reported since the last drain.
    fn drain_effects(&mut self) {
        let eye = self.game.player.eye_position();
        for effect in std::mem::take(&mut self.game.pending_effects) {
            match effect {
                Effect::Broke { cell, block } => {
                    self.swing.start();
                    self.particles.spawn_break(cell, block);
                    self.play(SoundKind::Break(material(block)));
                }
                Effect::Placed { block, .. } => {
                    self.swing.start();
                    self.play(SoundKind::Place(material(block)));
                }
                Effect::Dig { cell, block } => {
                    self.swing.start_if_idle();
                    self.particles.spawn_dig(cell, block, eye);
                    self.play(SoundKind::Dig(material(block)));
                }
            }
        }
    }

    fn play(&mut self, sound: SoundKind) {
        if let Some(audio) = self.audio.as_mut() {
            audio.play(sound);
        }
    }

    /// Starts the item-name fade when the hotbar selection changed.
    fn refresh_item_name(&mut self) {
        if self.game.selected != self.prev_selected {
            self.prev_selected = self.game.selected;
            self.show_item_name();
        }
    }

    pub(crate) fn show_item_name(&mut self) {
        self.item_name_until = Some(Instant::now() + Duration::from_millis(VIEW.item_name_ms));
    }

    /// Re-meshes edited chunks, then streams chunk data and meshes around
    /// the player within the budgets.
    pub(crate) fn sync_gpu(&mut self, data_budget: usize, mesh_budget: usize) {
        for key in self.game.pending_remesh.drain(..) {
            if self.meshed.contains(&key) {
                let mesh = build_mesh(&self.game.world, key.0, key.1);
                self.renderer.upload_chunk(key, &mesh);
            }
        }
        self.streamer.stream(
            &mut self.game,
            &mut self.renderer,
            &mut self.meshed,
            data_budget,
            mesh_budget,
        );
    }

    pub(crate) fn unload(&mut self) {
        Streamer::unload(&mut self.game, &mut self.renderer, &mut self.meshed);
    }

    pub(crate) fn meshed_count(&self) -> usize {
        self.meshed.len()
    }

    pub(crate) const fn mode_label(&self) -> &'static str {
        if self.game.player.flying {
            "fly"
        } else if self.game.player.on_ground {
            "ground"
        } else {
            "air"
        }
    }

    /// HUD parameters for the current state; `fps` is display-only. Owns
    /// everything it reports, so the renderer can upload it before the
    /// borrowing frame parameters exist.
    fn ui_state(&self, fps: u32) -> UiState<'static> {
        let game = &self.game;
        let now = Instant::now();
        let item_alpha = self.item_name_until.map_or(0.0, |t| {
            let left = t.saturating_duration_since(now).as_secs_f32();
            (left / VIEW.item_name_fade_s).clamp(0.0, 1.0)
        });
        UiState {
            overlay: self.overlay(),
            selected: game.selected,
            selected_name: def(HOTBAR[game.selected]).name,
            item_name_alpha: item_alpha,
            fps,
            pos: game.player.pos,
            mode: self.mode_label(),
            cursor: self.cursor,
            scroll: self.screens.scroll(),
            version: VERSION,
        }
    }

    /// Frame parameters for the current state; borrows the live particles.
    fn frame_params(&self) -> FrameParams<'_> {
        let overlay = self.overlay();
        let blend = self
            .menu_blend
            .unwrap_or_else(|| f32::from(overlay.is_some()));
        let game = &self.game;
        let eye = game.player.eye_position();
        let (width, height) = self.renderer.size();
        let aspect = width as f32 / height.max(1) as f32;
        let drift = blend * menu_drift(game.time);
        let (vp, inv_vp) = perspective_view(
            eye,
            game.player.yaw + drift,
            game.player.pitch,
            aspect,
            VIEW.fov,
            VIEW.far_plane,
        );
        let sun = sun_state(game.time);
        let highlight = overlay
            .is_none()
            .then(|| game.target().map(|h| [h.x, h.y, h.z]))
            .flatten();
        FrameParams {
            view_proj: vp,
            inv_view_proj: inv_vp,
            cam_pos: eye,
            sun_dir: sun.dir,
            sun_color: sun.color,
            zenith: sun.zenith,
            horizon: sun.horizon,
            highlight,
            fog_near: VIEW.fog_near(),
            fog_far: VIEW.fog_far(),
            hand: self.input.grabbed.then(|| HandParams {
                block: HOTBAR[game.selected],
                swing: self.swing.phase(),
            }),
            crack: game.breaking.map(|b| CrackParams {
                cell: b.cell,
                progress: b.progress,
            }),
            particles: self.particles.as_slice(),
        }
    }

    /// Presents one frame to the window.
    pub(crate) fn draw(&mut self, fps: u32) -> Result<(), String> {
        let ui = self.ui_state(fps);
        self.renderer.prepare_ui(Some(&ui));
        let frame = self.frame_params();
        self.renderer.draw(&frame, Some(&ui))
    }

    /// Renders one frame offscreen and saves it to `path` as PNG.
    pub(crate) fn capture(&mut self, path: &str) -> Result<(), String> {
        let ui = self.ui_state(0);
        self.renderer.prepare_ui(Some(&ui));
        let frame = self.frame_params();
        self.renderer.capture(&frame, Some(&ui), path)
    }
}

#[cfg(test)]
mod tests {
    use super::{MENU_BLEND_S, MENU_DRIFT_PERIOD_S, blend_toward, menu_drift};
    use ox_app::game::DAY_LENGTH;

    /// One rendered frame at 60 Hz, in simulated seconds.
    const FRAME_S: f32 = 1.0 / 60.0;

    #[test]
    fn menu_drift_sways_six_degrees_over_twenty_four_seconds() {
        assert!((MENU_DRIFT_PERIOD_S - 24.0).abs() < 1e-6, "period moved");
        let peak = menu_drift(MENU_DRIFT_PERIOD_S / 4.0);
        assert!((peak - 0.10).abs() < 1e-5, "peak yaw offset is {peak} rad");
        assert!(peak.to_degrees() < 8.0, "sway must stay a subtle drift");
    }

    #[test]
    fn menu_drift_is_continuous_where_the_day_wraps() {
        let before = menu_drift(DAY_LENGTH - FRAME_S);
        let after = menu_drift(0.0);
        assert!(
            (after - before).abs() < 0.002,
            "day wrap jumps {} rad",
            after - before
        );
    }

    #[test]
    fn menu_drift_never_jumps_within_one_frame() {
        let worst = (0..24_000)
            .map(|step| {
                let t = step as f32 * 0.1;
                (menu_drift(t + FRAME_S) - menu_drift(t)).abs()
            })
            .fold(0.0_f32, f32::max);
        assert!(worst < 0.002, "sway moves {worst} rad in one frame");
    }

    #[test]
    fn blend_reaches_the_target_within_its_fade_time() {
        let mut blend = 1.0;
        let steps = (MENU_BLEND_S / FRAME_S).ceil() as usize;
        for _ in 0..steps {
            blend = blend_toward(blend, 0.0, FRAME_S);
        }
        assert!(blend.abs() < 1e-6, "fade left {blend} after {steps} frames");
    }

    #[test]
    fn blend_never_overshoots_or_snaps() {
        let mut blend = 0.0;
        for _ in 0..600 {
            let next = blend_toward(blend, 1.0, FRAME_S);
            assert!(
                (next - blend) <= FRAME_S / MENU_BLEND_S + 1e-6,
                "blend jumped from {blend} to {next}"
            );
            assert!((0.0..=1.0).contains(&next), "blend left its range: {next}");
            blend = next;
        }
        assert!((blend - 1.0).abs() < 1e-6);
    }
}
