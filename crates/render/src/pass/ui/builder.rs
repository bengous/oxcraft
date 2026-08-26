//! Immediate-mode geometry accumulator: rectangles, atlas icons and bitmap
//! text into one flat vertex list, in pixel coordinates. It knows nothing
//! of the HUD, the menus or the GPU.

use std::ops::Range;

use bytemuck::{Pod, Zeroable};
use font8x8::UnicodeFonts;
use ox_core::atlas::{self, tile_uv};

use super::Rect;

/// One UI quad vertex in pixel coordinates with atlas UV and RGBA color.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct UiVertex {
    pos: [f32; 2],
    uv: [f32; 2],
    color: [f32; 4],
}

/// Accumulated UI geometry for one frame.
#[derive(Default)]
pub struct UiBuilder {
    /// Quad vertices.
    pub verts: Vec<UiVertex>,
    /// Triangle indices into [`UiBuilder::verts`].
    pub idx: Vec<u32>,
    /// Index ranges the pass draws under a scissor rectangle, in order.
    pub clips: Vec<(Range<u32>, Rect)>,
}

fn white_uv() -> [f32; 2] {
    tile_uv(atlas::T_WHITE, 0.5, 0.5)
}

impl UiBuilder {
    fn quad(&mut self, x: f32, y: f32, w: f32, h: f32, uv: [[f32; 2]; 4], color: [f32; 4]) {
        let base = self.verts.len() as u32;
        let corners = [[x, y], [x + w, y], [x + w, y + h], [x, y + h]];
        for k in 0..4 {
            self.verts.push(UiVertex {
                pos: corners[k],
                uv: uv[k],
                color,
            });
        }
        self.idx
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    /// Fills a solid-color rectangle.
    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) {
        let uv = white_uv();
        self.quad(x, y, w, h, [uv; 4], color);
    }

    /// Draws one atlas tile as a square icon.
    pub fn icon(&mut self, x: f32, y: f32, size: f32, tile: u8) {
        let uv = [
            tile_uv(tile, 0.0, 0.0),
            tile_uv(tile, 1.0, 0.0),
            tile_uv(tile, 1.0, 1.0),
            tile_uv(tile, 0.0, 1.0),
        ];
        self.quad(x, y, size, size, uv, [1.0, 1.0, 1.0, 1.0]);
    }

    /// Draws a line of bitmap text and returns the x coordinate after it.
    pub fn text(&mut self, text: &str, x: f32, y: f32, scale: f32, color: [f32; 4]) -> f32 {
        let mut cx = x;
        for ch in text.chars() {
            if let Some(bytes) = font8x8::BASIC_FONTS.get(ch) {
                for (row, bits) in bytes.iter().enumerate() {
                    for col in 0..8 {
                        if bits & (1 << col) != 0 {
                            self.rect(
                                cx + col as f32 * scale,
                                y + row as f32 * scale,
                                scale,
                                scale,
                                color,
                            );
                        }
                    }
                }
            }
            cx += 8.0 * scale;
        }
        cx
    }

    /// Width in pixels a string will occupy at `scale`.
    #[must_use]
    pub fn text_width(text: &str, scale: f32) -> f32 {
        text.chars().count() as f32 * 8.0 * scale
    }

    /// Draws through `body` with everything it emits clipped to `rect`.
    pub fn clipped(&mut self, rect: Rect, body: impl FnOnce(&mut Self)) {
        let start = self.idx.len() as u32;
        body(self);
        let end = self.idx.len() as u32;
        if end > start {
            self.clips.push((start..end, rect));
        }
    }

    /// Draws a line of text horizontally centered on `cx`.
    pub fn text_centered(&mut self, text: &str, cx: f32, y: f32, scale: f32, color: [f32; 4]) {
        let x = cx - Self::text_width(text, scale) / 2.0;
        self.text(text, x, y, scale, color);
    }

    /// Whether nothing would be drawn.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.idx.is_empty()
    }
}
