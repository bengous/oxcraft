//! Interactive shell: winit window, cursor grab, and the frame loop that
//! advances the session at a fixed timestep and draws it.

use std::sync::Arc;
use std::time::Instant;

use ox_app::harness::FIXED_DT;
use ox_app::input::KeyInput;
use ox_render::{ListScroll, MenuButton, Renderer, controls_scroll};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{DeviceEvent, ElementState, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{CursorGrabMode, Window, WindowId};

use crate::menu_input::{key_scroll_rows, key_starts_game, press_reaches_world, pressed_button};
use crate::screens::Screen;
use crate::session::Session;
use crate::view::VIEW;

/// List rows one wheel notch scrolls, as desktop toolkits do.
const WHEEL_ROWS: f32 = 3.0;

struct AppState {
    window: Option<Arc<Window>>,
    session: Option<Session>,
    fatal: Option<String>,
    last_frame: Option<Instant>,
    /// When a click last entered the world, for the entry grace.
    entered_world: Option<Instant>,
    accumulator: f32,
    unload_tick: u32,
    frames: u32,
    fps: u32,
    fps_since: Instant,
}

impl AppState {
    fn new() -> Self {
        Self {
            window: None,
            session: None,
            fatal: None,
            last_frame: None,
            entered_world: None,
            accumulator: 0.0,
            unload_tick: 0,
            frames: 0,
            fps: 0,
            fps_since: Instant::now(),
        }
    }

    fn fail(&mut self, event_loop: &ActiveEventLoop, message: String) {
        self.fatal = Some(message);
        event_loop.exit();
    }

    fn redraw(&mut self) {
        let (Some(window), Some(session)) = (self.window.clone(), self.session.as_mut()) else {
            return;
        };
        let now = Instant::now();
        let dt = self
            .last_frame
            .map_or(0.016, |t| now.duration_since(t).as_secs_f32())
            .min(0.1);
        self.last_frame = Some(now);
        self.accumulator += dt;
        let ticks = (self.accumulator / FIXED_DT) as usize;
        self.accumulator -= ticks as f32 * FIXED_DT;
        session.advance(ticks);
        session.sync_gpu(VIEW.data_budget, VIEW.mesh_budget);
        self.unload_tick += 1;
        if self.unload_tick >= VIEW.unload_interval {
            self.unload_tick = 0;
            session.unload();
        }
        let _ = session.draw(self.fps);

        self.frames += 1;
        let elapsed = self.fps_since.elapsed().as_secs_f32();
        if elapsed >= 1.0 {
            self.fps = (self.frames as f32 / elapsed).round() as u32;
            let pos = session.game.player.pos;
            window.set_title(&format!(
                "oxcraft — {} fps | xyz {:.0} {:.0} {:.0} | {}",
                self.fps,
                pos[0],
                pos[1],
                pos[2],
                session.mode_label()
            ));
            self.frames = 0;
            self.fps_since = Instant::now();
        }
        window.request_redraw();
    }

    fn playing(&self) -> bool {
        self.session.as_ref().is_some_and(Session::playing)
    }

    /// Applies the pointer mode the current screen calls for. Fails when the
    /// window system refuses both grab modes for the world: mouse-look is
    /// impossible without one.
    fn sync_pointer(&self) -> Result<(), String> {
        let (Some(window), Some(session)) = (&self.window, self.session.as_ref()) else {
            return Ok(());
        };
        if session.playing() {
            if let Err(locked) = window.set_cursor_grab(CursorGrabMode::Locked)
                && let Err(confined) = window.set_cursor_grab(CursorGrabMode::Confined)
            {
                return Err(format!(
                    "cursor grab refused: {locked}; confined: {confined}"
                ));
            }
            window.set_cursor_visible(false);
        } else {
            let _ = window.set_cursor_grab(CursorGrabMode::None);
            window.set_cursor_visible(true);
        }
        Ok(())
    }

    /// Scroll bounds of the list on the open screen.
    fn list_bounds(&self) -> Option<ListScroll> {
        let session = self.session.as_ref()?;
        let (width, height) = session.renderer.size();
        Some(controls_scroll(width as f32, height as f32))
    }

    /// Scrolls the open list; screens without a list ignore it.
    fn scroll_menu(&mut self, pixels: f32) {
        let Some(bounds) = self.list_bounds() else {
            return;
        };
        if let Some(session) = self.session.as_mut() {
            session.scroll_by(pixels, bounds.max);
        }
    }

    /// Runs one navigation step, then applies the pointer mode it calls for.
    /// A refused pointer lock ends the app.
    fn navigate(&mut self, event_loop: &ActiveEventLoop, step: fn(&mut Session)) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        step(session);
        if let Err(e) = self.sync_pointer() {
            self.fail(event_loop, e);
        }
    }
}

impl ApplicationHandler for AppState {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let (width, height) = VIEW.window_size;
        let attrs = Window::default_attributes()
            .with_title("oxcraft")
            .with_inner_size(LogicalSize::new(width, height));
        let window = match event_loop.create_window(attrs) {
            Ok(window) => Arc::new(window),
            Err(e) => {
                self.fail(event_loop, format!("create window: {e}"));
                return;
            }
        };
        let display = event_loop.owned_display_handle();
        let renderer = match pollster::block_on(Renderer::windowed(display, window.clone())) {
            Ok(renderer) => renderer,
            Err(e) => {
                self.fail(event_loop, format!("renderer init: {e}"));
                return;
            }
        };
        let mut session = Session::new(renderer);
        session.audio = crate::audio::Audio::open();
        self.session = Some(session);
        self.window = Some(window);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Focused(false) => {
                if self.playing() {
                    self.navigate(event_loop, Session::escape);
                }
            }
            // The buttons move under a pointer that stayed still, so the
            // last reported position no longer says which one it is over.
            WindowEvent::Resized(size) => {
                if let Some(session) = self.session.as_mut() {
                    session.renderer.resize(size.width, size.height);
                    session.cursor = None;
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let pressed = event.state == ElementState::Pressed;
                if pressed && matches!(event.logical_key, Key::Named(NamedKey::Escape)) {
                    self.navigate(event_loop, Session::escape);
                    return;
                }
                if !self.playing() {
                    if !pressed {
                        return;
                    }
                    if let Some(rows) = key_scroll_rows(&event.logical_key) {
                        let step = self.list_bounds().map_or(0.0, |b| b.step);
                        self.scroll_menu(rows * step);
                        return;
                    }
                    let overlay = self.session.as_ref().and_then(Session::overlay);
                    if overlay.is_some_and(|o| key_starts_game(&event.logical_key, o)) {
                        self.navigate(event_loop, Session::enter_world);
                    }
                    return;
                }
                if let Some(session) = self.session.as_mut() {
                    session.input.key(&KeyInput {
                        key: event.logical_key.clone(),
                        state: event.state,
                    });
                }
            }
            // TODO: in the world, the wheel should select a hotbar slot, as
            // `ox-alpha-1/src/main.js:216` does.
            WindowEvent::MouseWheel { delta, .. } => {
                let Some(bounds) = self.list_bounds() else {
                    return;
                };
                let pixels = match delta {
                    MouseScrollDelta::LineDelta(_, y) => -y * bounds.step * WHEEL_ROWS,
                    MouseScrollDelta::PixelDelta(p) => -p.y as f32,
                };
                self.scroll_menu(pixels);
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let pressed = state == ElementState::Pressed;
                if pressed && !self.playing() {
                    let hit = self.session.as_ref().and_then(|s| {
                        let (width, height) = s.renderer.size();
                        pressed_button(button, s.cursor, s.overlay()?, width as f32, height as f32)
                    });
                    match hit {
                        Some(MenuButton::Play) => self.navigate(event_loop, Session::enter_world),
                        Some(MenuButton::Controls) => {
                            self.navigate(event_loop, |s| s.open(Screen::controls()));
                        }
                        Some(MenuButton::Back) => self.navigate(event_loop, Session::escape),
                        None => {}
                    }
                    if self.playing() {
                        self.entered_world = Some(Instant::now());
                    }
                    return;
                }
                if press_reaches_world(self.entered_world.map(|t| t.elapsed()))
                    && let Some(session) = self.session.as_mut()
                {
                    session.input.mouse(button, pressed);
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                if let Some(session) = self.session.as_mut()
                    && !session.playing()
                {
                    session.cursor = Some([position.x as f32, position.y as f32]);
                }
            }
            WindowEvent::CursorLeft { .. } => {
                if let Some(session) = self.session.as_mut() {
                    session.cursor = None;
                }
            }
            WindowEvent::RedrawRequested => self.redraw(),
            _ => {}
        }
    }

    fn device_event(
        &mut self,
        _el: &ActiveEventLoop,
        _id: winit::event::DeviceId,
        event: DeviceEvent,
    ) {
        if let DeviceEvent::MouseMotion { delta } = event
            && let Some(session) = self.session.as_mut()
            && session.playing()
        {
            let player = &mut session.game.player;
            player.yaw -= delta.0 as f32 * VIEW.mouse_sensitivity;
            player.pitch -= delta.1 as f32 * VIEW.mouse_sensitivity;
            let lim = std::f32::consts::FRAC_PI_2 - 0.01;
            player.pitch = player.pitch.clamp(-lim, lim);
        }
    }

    fn about_to_wait(&mut self, _el: &ActiveEventLoop) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

/// Runs the interactive window until it closes.
pub(crate) fn run() -> Result<(), String> {
    let event_loop = EventLoop::new().map_err(|e| e.to_string())?;
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = AppState::new();
    event_loop.run_app(&mut app).map_err(|e| e.to_string())?;
    app.fatal.map_or(Ok(()), Err)
}
