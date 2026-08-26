//! The UI pass: lays the crosshair, hotbar and menus out for one frame,
//! then uploads and draws that geometry.

use ox_core::blocks::{HOTBAR, def};

use crate::gpu::Gpu;
use crate::pass::depth_stencil;

mod builder;
mod cache;
mod menu;

pub use builder::{UiBuilder, UiVertex};
use cache::{OverlayKey, Recorded};
pub use menu::{ListScroll, MenuButton, Rect, controls_scroll, menu_button};

/// Full-screen UI covering the game view. `None` draws the HUD instead.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Overlay {
    /// Start screen shown before the first grab.
    Title,
    /// Screen over a run already started. The world keeps running under it.
    Pause,
    /// Controls screen, opened from the title or pause screen.
    Controls,
}

impl Overlay {
    /// Buttons of this overlay's column, top to bottom.
    #[must_use]
    pub const fn buttons(self) -> &'static [MenuButton] {
        match self {
            Self::Title | Self::Pause => &[MenuButton::Play, MenuButton::Controls],
            Self::Controls => &[MenuButton::Back],
        }
    }
}

/// HUD state consumed by [`build`].
pub struct UiState<'a> {
    /// Which full-screen overlay covers the game, if any.
    pub overlay: Option<Overlay>,
    /// Selected hotbar slot index (0-based).
    pub selected: usize,
    /// Display name of the selected block.
    pub selected_name: &'a str,
    /// Fade alpha of the floating item-name label.
    pub item_name_alpha: f32,
    /// Frames per second measured by the app shell.
    pub fps: u32,
    /// Player position for the debug line.
    pub pos: [f32; 3],
    /// Current locomotion label ("fly"/"ground"/"air").
    pub mode: &'a str,
    /// Cursor position in window pixels while the pointer is free.
    pub cursor: Option<[f32; 2]>,
    /// Pixels the controls list is scrolled down by.
    pub scroll: f32,
    /// Version string shown on the overlays' bottom-left corner.
    pub version: &'a str,
}

/// Lays out the full UI for the current viewport size.
#[must_use]
pub fn build(ui: &UiState<'_>, width: f32, height: f32) -> UiBuilder {
    let mut b = UiBuilder::default();
    let (w, h) = (width, height);

    if let Some(overlay) = ui.overlay {
        menu::draw(&mut b, ui, overlay, w, h);
    } else {
        let fps_text = format!(
            "{} FPS   XYZ {:.0} {:.0} {:.0}   {}",
            ui.fps, ui.pos[0], ui.pos[1], ui.pos[2], ui.mode
        );
        b.text(&fps_text, 10.0, 10.0, 2.0, [1.0, 1.0, 1.0, 0.9]);

        let slot = 48.0;
        let gap = 4.0;
        let pad = 4.0;
        let count = 9.0;
        let bar_w = count * slot + (count - 1.0) * gap + pad * 2.0;
        let x0 = (w - bar_w) / 2.0;
        let y0 = h - slot - 16.0;
        b.rect(x0, y0, bar_w, slot + pad * 2.0, [0.03, 0.03, 0.05, 0.45]);
        for (i, &block) in HOTBAR.iter().enumerate() {
            let sx = x0 + pad + i as f32 * (slot + gap);
            let selected = i == ui.selected;
            let border = if selected {
                [1.0, 1.0, 1.0, 0.95]
            } else {
                [1.0, 1.0, 1.0, 0.28]
            };
            b.rect(sx, y0 + pad, slot, slot, border);
            b.rect(
                sx + 2.0,
                y0 + pad + 2.0,
                slot - 4.0,
                slot - 4.0,
                [0.0, 0.0, 0.0, 0.35],
            );
            let icon_tile = def(block).tiles[4];
            b.icon(sx + 8.0, y0 + pad + 8.0, slot - 16.0, icon_tile);
            let key = format!("{}", i + 1);
            b.text(&key, sx + 4.0, y0 + pad + 3.0, 1.0, [1.0, 1.0, 1.0, 0.75]);
        }

        if ui.item_name_alpha > 0.01 {
            b.text_centered(
                ui.selected_name,
                w / 2.0,
                y0 - 30.0,
                2.0,
                [1.0, 1.0, 1.0, ui.item_name_alpha],
            );
        }

        let cx = w / 2.0;
        let cy = h / 2.0;
        b.rect(cx - 9.0, cy - 1.0, 18.0, 2.0, [1.0, 1.0, 1.0, 0.85]);
        b.rect(cx - 1.0, cy - 9.0, 2.0, 18.0, [1.0, 1.0, 1.0, 0.85]);
    }
    b
}

/// UI pipeline with its fixed-size vertex, index and uniform buffers.
pub(crate) struct UiPass {
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    uniform_buf: wgpu::Buffer,
    bind: wgpu::BindGroup,
    recorded: Option<Recorded>,
}

impl UiPass {
    /// Creates the UI resources for `format`; the buffers are fixed-size
    /// and rewritten every frame during recording.
    pub(crate) fn new(gpu: &Gpu, format: wgpu::TextureFormat) -> Self {
        let uniform_buf = gpu.uniform_buffer("ui-uniforms", 16);
        let bind = gpu.atlas_bind_group("ui-bind", &uniform_buf);
        let module = gpu
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("ui-shader"),
                source: wgpu::ShaderSource::Wgsl(include_str!("../../shaders/ui.wgsl").into()),
            });
        let layout = gpu
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("ui-layout"),
                bind_group_layouts: &[Some(&gpu.scene_bgl)],
                immediate_size: 0,
            });
        let pipeline = gpu
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("ui-pipeline"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("vs_main"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &[wgpu::VertexBufferLayout {
                        array_stride: 32,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &[
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x2,
                                offset: 0,
                                shader_location: 0,
                            },
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x2,
                                offset: 8,
                                shader_location: 1,
                            },
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x4,
                                offset: 16,
                                shader_location: 2,
                            },
                        ],
                    }],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some("fs_main"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState {
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: Some(depth_stencil(false, wgpu::CompareFunction::Always)),
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            });
        let vertex_buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ui-vb"),
            size: 4 * 1024 * 1024,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let index_buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ui-ib"),
            size: 1024 * 1024,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            pipeline,
            vertex_buffer,
            index_buffer,
            uniform_buf,
            bind,
            recorded: None,
        }
    }

    /// Builds the HUD and uploads it into the fixed buffers; empty or
    /// oversized frames leave nothing to draw. Runs before the frame is
    /// encoded, the way `Renderer::upload_chunk` does.
    ///
    /// A menu the player leaves alone produces the same geometry every
    /// frame, so an unchanged overlay keeps the buffers as they stand and
    /// skips both the build and the upload.
    pub(crate) fn prepare(
        &mut self,
        queue: &wgpu::Queue,
        state: Option<&UiState<'_>>,
        width: f32,
        height: f32,
    ) {
        let Some(state) = state else { return };
        let key = OverlayKey::of(state, width, height);
        if Recorded::holds(self.recorded.as_ref(), key.as_ref()) {
            return;
        }
        let builder = build(state, width, height);
        let vert_bytes = std::mem::size_of_val(builder.verts.as_slice());
        let idx_bytes = std::mem::size_of_val(builder.idx.as_slice());
        if builder.is_empty()
            || vert_bytes > self.vertex_buffer.size() as usize
            || idx_bytes > self.index_buffer.size() as usize
        {
            self.recorded = None;
            return;
        }
        queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&builder.verts));
        queue.write_buffer(&self.index_buffer, 0, bytemuck::cast_slice(&builder.idx));
        let viewport: [f32; 4] = [width, height, 0.0, 0.0];
        queue.write_buffer(&self.uniform_buf, 0, bytemuck::bytes_of(&viewport));
        self.recorded = Some(Recorded { key, builder });
    }

    /// Records the draw for what [`Self::prepare`] left in the buffers. A
    /// frame that asks for no UI draws nothing and leaves the buffers alone.
    pub(crate) fn record(
        &self,
        rpass: &mut wgpu::RenderPass<'_>,
        state: Option<&UiState<'_>>,
        width: f32,
        height: f32,
    ) {
        if state.is_none() {
            return;
        }
        let Some(recorded) = self.recorded.as_ref() else {
            return;
        };
        rpass.set_pipeline(&self.pipeline);
        rpass.set_bind_group(0, &self.bind, &[]);
        rpass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        rpass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        draw_clipped(rpass, &recorded.builder, width, height);
    }
}

/// Draws the index list, switching the scissor rectangle over the ranges the
/// builder marked as clipped.
fn draw_clipped(rpass: &mut wgpu::RenderPass<'_>, builder: &UiBuilder, width: f32, height: f32) {
    let full = [0, 0, width as u32, height as u32];
    let total = builder.idx.len() as u32;
    let mut cursor = 0;
    for (range, rect) in &builder.clips {
        if range.start > cursor {
            rpass.set_scissor_rect(full[0], full[1], full[2], full[3]);
            rpass.draw_indexed(cursor..range.start, 0, 0..1);
        }
        let [x, y, w, h] = scissor(*rect, width, height);
        if w > 0 && h > 0 {
            rpass.set_scissor_rect(x, y, w, h);
            rpass.draw_indexed(range.clone(), 0, 0..1);
        }
        cursor = range.end;
    }
    rpass.set_scissor_rect(full[0], full[1], full[2], full[3]);
    if cursor < total {
        rpass.draw_indexed(cursor..total, 0, 0..1);
    }
}

/// A pixel rectangle as a scissor rectangle clamped to the render target.
fn scissor(r: Rect, width: f32, height: f32) -> [u32; 4] {
    let x0 = r.x.clamp(0.0, width);
    let y0 = r.y.clamp(0.0, height);
    let x1 = (r.x + r.w).clamp(x0, width);
    let y1 = (r.y + r.h).clamp(y0, height);
    [x0 as u32, y0 as u32, (x1 - x0) as u32, (y1 - y0) as u32]
}

#[cfg(test)]
mod tests {
    use super::{Overlay, UiBuilder, UiState, build, menu_button};

    const SIZE: (f32, f32) = (1280.0, 720.0);

    /// Geometry digest of each menu at [`SIZE`], the oracle for the overlay
    /// layout. `docs/oxcraft-menu.png` is a README image no job compares,
    /// and a lavapipe golden holds for one Mesa build only, while `build` is
    /// device-free and answers the same number everywhere. A commit that
    /// moves a menu pixel moves one of these and names it on its `Oracles:`
    /// line.
    ///
    /// The title and pause screens share a digest: today they draw the same
    /// title, splash and buttons.
    const MENU_DIGESTS: [(&str, u64); 4] = [
        ("title", 0x63ef_f1b3_7d77_2c83),
        ("title with the pointer on PLAY", 0x1848_a1b7_79b6_9157),
        ("pause", 0x63ef_f1b3_7d77_2c83),
        ("controls", 0xf35e_a36c_87f3_fdcf),
    ];

    fn state(overlay: Option<Overlay>) -> UiState<'static> {
        UiState {
            overlay,
            selected: 0,
            selected_name: "GRASS",
            item_name_alpha: 0.0,
            fps: 60,
            pos: [0.0, 70.0, 0.0],
            mode: "ground",
            cursor: None,
            scroll: 0.0,
            version: "oxcraft 0.1.0",
        }
    }

    /// One FNV-1a step per byte.
    fn eat(hash: u64, bytes: &[u8]) -> u64 {
        bytes.iter().fold(hash, |acc, byte| {
            (acc ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
        })
    }

    /// FNV-1a over every vertex, index and clip rectangle the build emitted.
    fn digest(b: &UiBuilder) -> u64 {
        let mut hash = eat(0xcbf2_9ce4_8422_2325, bytemuck::cast_slice(&b.verts));
        hash = eat(hash, bytemuck::cast_slice(&b.idx));
        for (range, rect) in &b.clips {
            hash = eat(hash, &range.start.to_le_bytes());
            hash = eat(hash, &range.end.to_le_bytes());
            for edge in [rect.x, rect.y, rect.w, rect.h] {
                hash = eat(hash, &edge.to_le_bytes());
            }
        }
        hash
    }

    /// The four pinned menus, in [`MENU_DIGESTS`] order.
    fn menus() -> [UiState<'static>; 4] {
        let hovered = menu_button(SIZE.0, SIZE.1, Overlay::Title, 0);
        let mut on_play = state(Some(Overlay::Title));
        on_play.cursor = Some([hovered.x + hovered.w / 2.0, hovered.y + hovered.h / 2.0]);
        [
            state(Some(Overlay::Title)),
            on_play,
            state(Some(Overlay::Pause)),
            state(Some(Overlay::Controls)),
        ]
    }

    #[test]
    fn every_menu_draws_the_pinned_geometry() {
        for (menu, (name, pinned)) in menus().into_iter().zip(MENU_DIGESTS) {
            let got = digest(&build(&menu, SIZE.0, SIZE.1));
            assert_eq!(got, pinned, "the {name} menu moved; digest is {got:#018x}");
        }
    }

    #[test]
    fn hovering_a_button_moves_the_geometry() {
        let [plain, on_play, ..] = menus();
        assert_ne!(
            digest(&build(&plain, SIZE.0, SIZE.1)),
            digest(&build(&on_play, SIZE.0, SIZE.1)),
            "the hovered button looks the same as the idle one"
        );
    }

    #[test]
    fn a_keyed_overlay_draws_the_same_geometry_twice() {
        let title = state(Some(Overlay::Title));
        let first = build(&title, SIZE.0, SIZE.1);
        let second = build(&title, SIZE.0, SIZE.1);
        assert_eq!(first.idx, second.idx);
        assert_eq!(first.verts.len(), second.verts.len());
        assert!(!first.is_empty());
    }
}
